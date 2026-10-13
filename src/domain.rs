#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceCapability {
    OnOff,
    Brightness,
    ColorTemperature,
    Color,
}

impl DeviceCapability {
    pub fn label(self) -> &'static str {
        match self {
            Self::OnOff => "ligar/desligar",
            Self::Brightness => "brilho",
            Self::ColorTemperature => "temperatura de cor",
            Self::Color => "cor",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatterDevice {
    pub endpoint_id: u16,
    pub name: String,
    pub capabilities: Vec<DeviceCapability>,
}

/// Luz exposta por um provider, independente do protocolo de origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightDevice {
    pub provider_id: String,
    pub node_id: u64,
    pub endpoint_id: u16,
    pub parent_endpoint: Option<u16>,
    pub display_name: String,
    pub room: Option<String>,
    /// Botão de rotina em vez de luz física.
    pub is_action: bool,
    /// Endpoint que representa a mesma luz (paralelo virtual).
    pub twin_endpoint: Option<u16>,
    pub on: bool,
    pub capabilities: Vec<DeviceCapability>,
    /// Rótulos brutos (Fixed Label + User Label) informados pela bridge.
    pub labels: Vec<(String, String)>,
}

impl LightDevice {
    pub fn summary(&self) -> String {
        let room = self
            .room
            .as_deref()
            .map(|room| format!(" ({room})"))
            .unwrap_or_default();
        let id = match self.twin_endpoint {
            Some(twin) => format!(
                "{}+{}",
                self.endpoint_id.min(twin),
                self.endpoint_id.max(twin)
            ),
            None => self.endpoint_id.to_string(),
        };
        let state = if self.is_action {
            "ação"
        } else if self.on {
            "ligada"
        } else {
            "desligada"
        };
        format!("[{id}] {}{room} — {state}", self.display_name)
    }

    /// Aplica o apelido/cômodo local, quando definido.
    pub fn apply_alias(&mut self, aliases: &crate::aliases::Aliases) {
        if let Some(alias) = aliases.get(self.endpoint_id) {
            if !alias.name.is_empty() {
                self.display_name = alias.name.clone();
            }
            if !alias.room.is_empty() {
                self.room = Some(alias.room.clone());
            }
            self.is_action = alias.action;
            self.twin_endpoint = alias.twin;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeInfo {
    pub node_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommissioningCode(String);

impl CommissioningCode {
    pub fn parse(value: &str) -> Result<Self, String> {
        let normalized = value.trim();
        let manual_without_separators = normalized.replace(['-', ' '], "");
        if normalized.is_empty() {
            return Err("o codigo Matter nao pode estar vazio".into());
        }

        // Códigos QR Matter normalmente começam por MT:. Códigos manuais são
        // numéricos; aceitamos ambos para não bloquear um modelo de hub válido.
        let is_qr = normalized.starts_with("MT:") && normalized.len() > 3;
        let is_manual = manual_without_separators
            .chars()
            .all(|character| character.is_ascii_digit())
            && (10..=21).contains(&manual_without_separators.len());

        if !is_qr && !is_manual {
            return Err(
                "use um codigo QR Matter (MT:...) ou o codigo manual numerico do gateway".into(),
            );
        }

        Ok(Self(if is_manual {
            manual_without_separators
        } else {
            normalized.to_owned()
        }))
    }

    pub fn redacted(&self) -> String {
        if self.0.starts_with("MT:") {
            "MT:[oculto]".into()
        } else {
            "[codigo manual oculto]".into()
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{CommissioningCode, DeviceCapability, LightDevice};

    #[test]
    fn light_summary_is_readable() {
        let light = LightDevice {
            provider_id: "matter".into(),
            node_id: 2,
            endpoint_id: 13,
            parent_endpoint: None,
            display_name: "Endpoint 13".into(),
            room: None,
            is_action: false,
            twin_endpoint: None,
            on: true,
            capabilities: vec![DeviceCapability::OnOff],
            labels: vec![],
        };
        assert_eq!(light.summary(), "[13] Endpoint 13 — ligada");
    }

    #[test]
    fn accepts_matter_qr_payload() {
        assert!(CommissioningCode::parse("MT:ABC123").is_ok());
    }

    #[test]
    fn rejects_unrecognized_code() {
        assert!(CommissioningCode::parse("not-a-matter-code").is_err());
    }

    #[test]
    fn accepts_hyphenated_manual_code() {
        assert!(CommissioningCode::parse("1234-567-8901").is_ok());
    }
}

