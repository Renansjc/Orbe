use serde::Serialize;

use crate::{aliases::Aliases, domain::LightDevice};

/// Item exibido na interface: uma luz (possivelmente com 2 endpoints em
/// paralelo) ou uma ação/rotina.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ItemView {
    /// Endpoint usado para enviar comandos (o menor, em paralelos).
    pub id: u16,
    pub endpoints: Vec<u16>,
    pub name: String,
    pub room: Option<String>,
    pub on: bool,
    /// "light" ou "action".
    pub kind: &'static str,
    /// Paralelo cujos dois lados discordam no estado.
    pub diverged: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PanelView {
    pub id: Option<u16>,
    pub name: String,
    pub room: Option<String>,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub panels: Vec<PanelView>,
    pub lights_on: usize,
    pub lights_total: usize,
}

/// Agrupa as luzes por painel físico, aplica apelidos e funde paralelos.
pub fn build_snapshot(mut lights: Vec<LightDevice>, aliases: &Aliases) -> Snapshot {
    for light in &mut lights {
        light.apply_alias(aliases);
    }
    lights.sort_by_key(|light| (light.parent_endpoint, light.endpoint_id));

    let mut panels: Vec<PanelView> = Vec::new();
    let (mut lights_on, mut lights_total) = (0, 0);
    for light in &lights {
        // Em paralelo, o menor endpoint representa o par.
        if light.twin_endpoint.is_some_and(|twin| twin < light.endpoint_id) {
            continue;
        }
        let diverged = light.twin_endpoint.is_some_and(|twin| {
            lights
                .iter()
                .any(|other| other.endpoint_id == twin && other.on != light.on)
        });
        let mut endpoints = vec![light.endpoint_id];
        endpoints.extend(light.twin_endpoint);

        if panels.last().is_none_or(|panel| panel.id != light.parent_endpoint) {
            let alias = light.parent_endpoint.and_then(|id| aliases.get(id));
            let name = match (alias.filter(|a| !a.name.is_empty()), light.parent_endpoint) {
                (Some(alias), _) => alias.name.clone(),
                (None, Some(id)) => format!("Painel {id}"),
                (None, None) => "Sem painel".to_owned(),
            };
            panels.push(PanelView {
                id: light.parent_endpoint,
                name,
                room: alias.map(|a| a.room.clone()).filter(|room| !room.is_empty()),
                items: Vec::new(),
            });
        }
        if !light.is_action {
            lights_total += 1;
            lights_on += usize::from(light.on);
        }
        panels
            .last_mut()
            .expect("painel criado acima")
            .items
            .push(ItemView {
                id: light.endpoint_id,
                endpoints,
                name: light.display_name.clone(),
                room: light.room.clone(),
                on: light.on,
                kind: if light.is_action { "action" } else { "light" },
                diverged,
            });
    }
    Snapshot {
        panels,
        lights_on,
        lights_total,
    }
}

#[cfg(test)]
mod tests {
    use super::build_snapshot;
    use crate::{
        aliases::Aliases,
        domain::{DeviceCapability, LightDevice},
    };

    fn light(endpoint: u16, parent: u16, on: bool) -> LightDevice {
        LightDevice {
            provider_id: "matter".into(),
            node_id: 2,
            endpoint_id: endpoint,
            parent_endpoint: Some(parent),
            display_name: format!("Endpoint {endpoint}"),
            room: None,
            is_action: false,
            twin_endpoint: None,
            on,
            capabilities: vec![DeviceCapability::OnOff],
            labels: vec![],
        }
    }

    #[test]
    fn groups_by_panel_merges_parallel_and_counts_actions_out() {
        let mut aliases = Aliases::default();
        aliases.set(12, "Entrada", "Sala");
        aliases.link(14, 23);
        aliases.set_action(15, true);
        let lights = vec![
            light(14, 12, true),
            light(15, 12, false),
            light(23, 21, true),
            light(22, 21, false),
        ];
        let snapshot = build_snapshot(lights, &aliases);

        assert_eq!(snapshot.panels.len(), 2);
        assert_eq!(snapshot.panels[0].name, "Entrada");
        assert_eq!(snapshot.panels[1].name, "Painel 21");
        // 14+23 aparece uma vez, no painel do menor endpoint.
        let all: Vec<_> = snapshot.panels.iter().flat_map(|p| &p.items).collect();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].endpoints, vec![14, 23]);
        assert!(!all[0].diverged);
        assert_eq!(all[1].kind, "action");
        assert_eq!((snapshot.lights_on, snapshot.lights_total), (1, 2));
    }

    #[test]
    fn flags_diverged_parallel() {
        let mut aliases = Aliases::default();
        aliases.link(14, 23);
        let snapshot = build_snapshot(vec![light(14, 12, true), light(23, 21, false)], &aliases);
        assert!(snapshot.panels[0].items[0].diverged);
    }
}
