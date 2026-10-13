use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use matter_controller::{
    AttestationTrust, CommandPath, FabricConfig, FileStore, ImStatus, InvokeResult,
    MatterController, MatterTime, ReadPath, Value,
};
use mdns_sd::{RecvTimeoutError, ServiceDaemon, ServiceEvent};

use crate::{
    domain::{BridgeInfo, DeviceCapability, LightDevice},
    state,
};

#[derive(Debug)]
pub enum BackendError {
    MissingTrustRoots { paa: String, cd: String },
    Runtime(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTrustRoots { paa, cd } => write!(
                formatter,
                "faltam as raizes Matter (.der). PAA: {paa}; CD: {cd}"
            ),
            Self::Runtime(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for BackendError {}

fn has_der_files(path: &Path) -> bool {
    path.read_dir().is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("der"))
        })
    })
}

async fn controller() -> Result<MatterController, BackendError> {
    let paa_dir = state::paa_roots_dir();
    let cd_dir = state::cd_roots_dir();
    if !has_der_files(&paa_dir) || !has_der_files(&cd_dir) {
        return Err(BackendError::MissingTrustRoots {
            paa: paa_dir.display().to_string(),
            cd: cd_dir.display().to_string(),
        });
    }

    let store = Arc::new(FileStore::new(state::controller_store_path()));
    let trust = AttestationTrust::from_dirs(&paa_dir, &cd_dir)
        .map_err(|error| BackendError::Runtime(error.to_string()))?;
    let controller = MatterController::builder(store)
        .attestation_trust(trust)
        .build()
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?;

    if controller
        .fabrics()
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?
        .is_empty()
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| BackendError::Runtime(error.to_string()))?
            .as_secs();
        controller
            .create_fabric(FabricConfig::new(
                1,
                1,
                1,
                (
                    MatterTime::from_unix_secs(now.saturating_sub(3600)),
                    MatterTime::NO_EXPIRY,
                ),
            ))
            .await
            .map_err(|error| BackendError::Runtime(error.to_string()))?;
    }
    Ok(controller)
}

pub async fn commission(setup_code: &str) -> Result<BridgeInfo, BackendError> {
    let controller = controller().await?;
    let node = controller
        .commission(setup_code, Some("NovaDigital MT-01".into()))
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?;
    Ok(BridgeInfo {
        node_id: node.node_id,
        name: node.label.unwrap_or_else(|| "Matter bridge".into()),
    })
}

pub async fn bridges() -> Result<Vec<BridgeInfo>, BackendError> {
    let controller = controller().await?;
    Ok(controller
        .nodes()
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?
        .into_iter()
        .map(|node| BridgeInfo {
            node_id: node.node_id,
            name: node.label.unwrap_or_else(|| "Matter bridge".into()),
        })
        .collect())
}

pub async fn inspect_bridges() -> Result<Vec<String>, BackendError> {
    let controller = controller().await?;
    let mut reports = Vec::new();
    for bridge in controller
        .nodes()
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?
    {
        let root_values = controller
            .node(bridge.node_id)
            .read(&[ReadPath::cluster(0, 0x001D)])
            .await
            .map_err(|error| BackendError::Runtime(error.to_string()))?;
        let endpoints = root_values
            .iter()
            .find(|(path, _)| path.attribute == 3)
            .map(|(_, value)| unsigned_array(value))
            .unwrap_or_default();

        reports.push(format!("Node {}: {} endpoints filhos", bridge.node_id, endpoints.len()));
        for endpoint in endpoints {
            let values = controller
                .node(bridge.node_id)
                .read(&[ReadPath::cluster(endpoint, 0x001D)])
                .await
                .map_err(|error| BackendError::Runtime(error.to_string()))?;
            let clusters = values
                .iter()
                .find(|(path, _)| path.attribute == 1)
                .map(|(_, value)| unsigned_array(value))
                .unwrap_or_default();
            reports.push(format!(
                "  endpoint {endpoint}: {}",
                clusters
                    .iter()
                    .map(|cluster| format!("0x{cluster:04X}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    Ok(reports)
}

/// Lista endpoints On/Off e os metadados de rótulo que a bridge fornece.
/// Esta operação é somente leitura: nenhuma luz é alterada.
pub async fn inspect_lights() -> Result<Vec<String>, BackendError> {
    let controller = controller().await?;
    let mut reports = Vec::new();
    for bridge in controller
        .nodes()
        .await
        .map_err(|error| BackendError::Runtime(error.to_string()))?
    {
        let root_values = controller
            .node(bridge.node_id)
            .read(&[ReadPath::cluster(0, 0x001D)])
            .await
            .map_err(|error| BackendError::Runtime(error.to_string()))?;
        let endpoints = root_values
            .iter()
            .find(|(path, _)| path.attribute == 3)
            .map(|(_, value)| unsigned_array(value))
            .unwrap_or_default();

        reports.push(format!("Bridge node {}", bridge.node_id));
        for endpoint in endpoints {
            let descriptor = controller
                .node(bridge.node_id)
                .read(&[ReadPath::cluster(endpoint, 0x001D)])
                .await
                .map_err(|error| BackendError::Runtime(error.to_string()))?;
            let clusters = descriptor
                .iter()
                .find(|(path, _)| path.attribute == 1)
                .map(|(_, value)| unsigned_array(value))
                .unwrap_or_default();
            if !clusters.contains(&0x0006) {
                continue;
            }

            let values = controller
                .node(bridge.node_id)
                .read(&[
                    ReadPath::cluster(endpoint, 0x0006),
                    ReadPath::cluster(endpoint, 0x0040),
                    ReadPath::cluster(endpoint, 0x0041),
                ])
                .await
                .map_err(|error| BackendError::Runtime(error.to_string()))?;
            reports.push(format!("  endpoint {endpoint}: {values:#?}"));
        }
    }
    Ok(reports)
}

/// Lê as luzes (endpoints com On/Off) como `LightDevice`. Somente leitura.
/// O nome vem do User Label "name", se existir; senão `Endpoint N`.
pub async fn list_lights() -> Result<Vec<LightDevice>, BackendError> {
    let controller = controller().await?;
    lights_with(&controller).await
}

async fn lights_with(controller: &MatterController) -> Result<Vec<LightDevice>, BackendError> {
    let mut lights = Vec::new();
    for bridge in controller.nodes().await.map_err(runtime)? {
        let node = controller.node(bridge.node_id);
        let mut parents = std::collections::HashMap::new();
        let root = node.read(&[ReadPath::cluster(0, 0x001D)]).await.map_err(runtime)?;
        let endpoints = root
            .iter()
            .find(|(path, _)| path.attribute == 3)
            .map(|(_, value)| unsigned_array(value))
            .unwrap_or_default();

        for endpoint in endpoints {
            let descriptor = node
                .read(&[ReadPath::cluster(endpoint, 0x001D)])
                .await
                .map_err(runtime)?;
            let clusters = descriptor
                .iter()
                .find(|(path, _)| path.attribute == 1)
                .map(|(_, value)| unsigned_array(value))
                .unwrap_or_default();
            if clusters.contains(&0x0039) {
                // Painel/dispositivo ponte: suas partes são os botões filhos.
                let parts = descriptor
                    .iter()
                    .find(|(path, _)| path.attribute == 3)
                    .map(|(_, value)| unsigned_array(value))
                    .unwrap_or_default();
                for part in parts {
                    parents.insert(part, endpoint);
                }
            }
            if !clusters.contains(&0x0006) {
                continue;
            }

            let values = node
                .read(&[
                    ReadPath::cluster(endpoint, 0x0006),
                    ReadPath::cluster(endpoint, 0x0040),
                    ReadPath::cluster(endpoint, 0x0041),
                ])
                .await
                .map_err(runtime)?;
            let on = values.iter().any(|(path, value)| {
                path.cluster == 0x0006 && path.attribute == 0 && matches!(value, Value::Bool(true))
            });
            let mut labels = Vec::new();
            for (path, value) in &values {
                if (path.cluster == 0x0040 || path.cluster == 0x0041) && path.attribute == 0 {
                    labels.extend(label_pairs(value));
                }
            }
            let display_name = labels
                .iter()
                .find(|(key, value)| key == "name" && !value.is_empty())
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| format!("Endpoint {endpoint}"));
            lights.push(LightDevice {
                provider_id: "matter".into(),
                node_id: bridge.node_id,
                endpoint_id: endpoint,
                parent_endpoint: parents.get(&endpoint).copied(),
                display_name,
                room: None,
                is_action: false,
                twin_endpoint: None,
                on,
                capabilities: vec![DeviceCapability::OnOff],
                labels,
            });
        }
    }
    Ok(lights)
}

/// Liga ou desliga uma luz (cluster On/Off 0x0006, comandos Off=0x00 / On=0x01)
/// e devolve o estado relido do dispositivo. ALTERA HARDWARE: o chamador deve
/// ter obtido confirmação do usuário. Só aceita endpoints que expõem On/Off.
pub async fn set_light(endpoint: u16, on: bool) -> Result<LightDevice, BackendError> {
    let controller = controller().await?;
    set_light_with(&controller, endpoint, on).await
}

async fn set_light_with(
    controller: &MatterController,
    endpoint: u16,
    on: bool,
) -> Result<LightDevice, BackendError> {
    let known = lights_with(controller).await?;
    let light = known
        .iter()
        .find(|light| light.endpoint_id == endpoint)
        .ok_or_else(|| BackendError::Runtime(format!("endpoint {endpoint} nao e uma luz On/Off")))?;

    let result = controller
        .node(light.node_id)
        .invoke(
            CommandPath {
                endpoint,
                cluster: 0x0006,
                command: if on { 0x01 } else { 0x00 },
            },
            Value::Structure(Vec::new()),
        )
        .await
        .map_err(runtime)?;
    match result {
        InvokeResult::Status(ImStatus::Failure(code)) => {
            return Err(BackendError::Runtime(format!(
                "o dispositivo recusou o comando (status 0x{code:02X})"
            )));
        }
        InvokeResult::Status(ImStatus::Success) | InvokeResult::Data { .. } => {}
        _ => {
            return Err(BackendError::Runtime(
                "resposta inesperada do dispositivo ao comando".into(),
            ));
        }
    }

    lights_with(controller)
        .await?
        .into_iter()
        .find(|light| light.endpoint_id == endpoint)
        .ok_or_else(|| BackendError::Runtime("luz nao encontrada apos o comando".into()))
}

/// Conexão persistente com o hub, para uso por interfaces (evita reconectar
/// a cada operação). A topologia é lida uma vez; depois só o estado On/Off.
pub struct Hub {
    controller: MatterController,
    topology: tokio::sync::Mutex<Option<Vec<LightDevice>>>,
}

impl Hub {
    pub async fn connect() -> Result<Self, BackendError> {
        Ok(Self {
            controller: controller().await?,
            topology: tokio::sync::Mutex::new(None),
        })
    }

    /// Luzes com estado atual. A primeira chamada lê a topologia completa.
    pub async fn lights(&self) -> Result<Vec<LightDevice>, BackendError> {
        let mut cache = self.topology.lock().await;
        let Some(known) = cache.as_mut() else {
            let fresh = lights_with(&self.controller).await?;
            *cache = Some(fresh.clone());
            return Ok(fresh);
        };
        for light in known.iter_mut() {
            let values = self
                .controller
                .node(light.node_id)
                .read(&[ReadPath::cluster(light.endpoint_id, 0x0006)])
                .await
                .map_err(runtime)?;
            light.on = values.iter().any(|(path, value)| {
                path.cluster == 0x0006 && path.attribute == 0 && matches!(value, Value::Bool(true))
            });
        }
        Ok(known.clone())
    }

    /// ALTERA HARDWARE: o chamador deve ter a intenção explícita do usuário.
    pub async fn set_light(&self, endpoint: u16, on: bool) -> Result<LightDevice, BackendError> {
        let light = set_light_with(&self.controller, endpoint, on).await?;
        if let Some(known) = self.topology.lock().await.as_mut() {
            if let Some(entry) = known.iter_mut().find(|l| l.endpoint_id == endpoint) {
                entry.on = light.on;
            }
        }
        Ok(light)
    }
}

/// Diagnóstico somente leitura: Bridged Device Basic Information (0x0039) e
/// Descriptor (0x001D) de todos os endpoints, em formato compacto.
pub async fn inspect_names() -> Result<Vec<String>, BackendError> {
    let controller = controller().await?;
    let mut out = Vec::new();
    for bridge in controller.nodes().await.map_err(runtime)? {
        let node = controller.node(bridge.node_id);
        let root = node.read(&[ReadPath::cluster(0, 0x001D)]).await.map_err(runtime)?;
        let endpoints = root
            .iter()
            .find(|(path, _)| path.attribute == 3)
            .map(|(_, value)| unsigned_array(value))
            .unwrap_or_default();
        for endpoint in endpoints {
            for cluster in [0x001Du32, 0x0039] {
                let values = match node.read(&[ReadPath::cluster(endpoint, cluster)]).await {
                    Ok(values) => values,
                    Err(_) => continue,
                };
                for (path, value) in values {
                    if path.attribute >= 0xFFF0 {
                        continue;
                    }
                    out.push(format!(
                        "ep{endpoint} 0x{cluster:04X} attr {}: {}",
                        path.attribute,
                        compact(&value)
                    ));
                }
            }
        }
    }
    Ok(out)
}

fn compact(value: &Value) -> String {
    format!("{value:?}")
}

fn runtime(error: impl std::fmt::Display) -> BackendError {
    BackendError::Runtime(error.to_string())
}

/// Converte uma lista de LabelStruct (label=ctx 0, value=ctx 1) em pares.
fn label_pairs(value: &Value) -> Vec<(String, String)> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Structure(fields) = item else {
                return None;
            };
            let field = |index: u8| {
                fields.iter().find_map(|(tag, value)| match (tag, value) {
                    (matter_codec::Tag::Context(i), Value::Utf8(text)) if *i == index => {
                        Some(text.clone())
                    }
                    _ => None,
                })
            };
            Some((field(0)?, field(1).unwrap_or_default()))
        })
        .collect()
}

fn unsigned_array(value: &Value) -> Vec<u16> {
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(|value| match value {
                Value::Uint(number) => u16::try_from(*number).ok(),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Procura bridges com uma janela de comissionamento Matter ativa.
pub fn discover_commissionable(timeout: std::time::Duration) -> Result<Vec<String>, BackendError> {
    let daemon = ServiceDaemon::new().map_err(|error| BackendError::Runtime(error.to_string()))?;
    let receiver = daemon
        .browse("_matterc._udp.local.")
        .map_err(|error| BackendError::Runtime(error.to_string()))?;
    let deadline = std::time::Instant::now() + timeout;
    let mut services = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => services.push(format!(
                "{} — {}:{} ({:?})",
                info.get_fullname(),
                info.get_hostname(),
                info.get_port(),
                info.get_addresses()
            )),
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => break,
            Err(error) => return Err(BackendError::Runtime(error.to_string())),
        }
    }
    daemon.shutdown().map_err(|error| BackendError::Runtime(error.to_string()))?;
    Ok(services)
}
