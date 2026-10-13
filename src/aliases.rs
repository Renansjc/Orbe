use std::{collections::BTreeMap, fs, io, path::PathBuf};

use crate::state;

/// Apelido e cômodo definidos pelo usuário para um endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub room: String,
    /// Botão de rotina (ligar/desligar várias luzes), não uma luz física.
    pub action: bool,
    /// Outro endpoint que representa a mesma luz (paralelo virtual).
    pub twin: Option<u16>,
}

/// Aliases por endpoint, guardados localmente (fora do Git e das chaves Matter).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aliases(BTreeMap<u16, Alias>);

fn clean(text: &str) -> String {
    text.replace(['\t', '\r', '\n'], " ").trim().to_owned()
}

impl Aliases {
    pub fn path() -> PathBuf {
        state::app_data_dir().join("aliases.tsv")
    }

    pub fn get(&self, endpoint: u16) -> Option<&Alias> {
        self.0.get(&endpoint)
    }

    /// Define nome e cômodo, preservando tipo e vínculo já configurados.
    pub fn set(&mut self, endpoint: u16, name: &str, room: &str) {
        let alias = self.0.entry(endpoint).or_default();
        alias.name = clean(name);
        alias.room = clean(room);
    }

    pub fn set_action(&mut self, endpoint: u16, action: bool) {
        self.0.entry(endpoint).or_default().action = action;
    }

    /// Vincula dois endpoints como a mesma luz (relação simétrica).
    pub fn link(&mut self, a: u16, b: u16) {
        self.0.entry(a).or_default().twin = Some(b);
        self.0.entry(b).or_default().twin = Some(a);
    }

    pub fn parse(contents: &str) -> Self {
        let mut map = BTreeMap::new();
        for line in contents.lines() {
            let mut parts = line.split('\t');
            let (Some(endpoint), Some(name)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(endpoint) = endpoint.parse::<u16>() else {
                continue;
            };
            map.insert(
                endpoint,
                Alias {
                    name: name.to_owned(),
                    room: parts.next().unwrap_or_default().to_owned(),
                    action: parts.next() == Some("acao"),
                    twin: parts.next().and_then(|twin| twin.parse().ok()),
                },
            );
        }
        Self(map)
    }

    pub fn serialize(&self) -> String {
        self.0
            .iter()
            .map(|(endpoint, alias)| {
                format!(
                    "{endpoint}\t{}\t{}\t{}\t{}\n",
                    alias.name,
                    alias.room,
                    if alias.action { "acao" } else { "luz" },
                    alias.twin.map(|twin| twin.to_string()).unwrap_or_default()
                )
            })
            .collect()
    }

    pub fn load() -> io::Result<Self> {
        match fs::read_to_string(Self::path()) {
            Ok(contents) => Ok(Self::parse(&contents)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.serialize())
    }
}

#[cfg(test)]
mod tests {
    use super::Aliases;

    #[test]
    fn round_trips_and_sanitizes() {
        let mut aliases = Aliases::default();
        aliases.set(13, "Luz\tprincipal", "Sala");
        aliases.set(14, "Fita", "");
        let parsed = Aliases::parse(&aliases.serialize());
        assert_eq!(parsed, aliases);
        assert_eq!(parsed.get(13).unwrap().name, "Luz principal");
        assert_eq!(parsed.get(14).unwrap().room, "");
    }

    #[test]
    fn keeps_kind_and_link_when_renaming() {
        let mut aliases = Aliases::default();
        aliases.link(14, 23);
        aliases.set_action(15, true);
        aliases.set(14, "Sala", "Sala");
        aliases.set(15, "Liga sala", "Sala");
        let parsed = Aliases::parse(&aliases.serialize());
        assert_eq!(parsed.get(14).unwrap().twin, Some(23));
        assert_eq!(parsed.get(23).unwrap().twin, Some(14));
        assert!(parsed.get(15).unwrap().action);
        assert!(!parsed.get(14).unwrap().action);
    }

    #[test]
    fn reads_old_two_column_file() {
        let parsed = Aliases::parse("13\tJantar\tSala\n");
        let alias = parsed.get(13).unwrap();
        assert!(!alias.action && alias.twin.is_none());
    }

    #[test]
    fn ignores_malformed_lines() {
        let parsed = Aliases::parse("abc\tx\n\n15\tLuz\tCozinha\n");
        assert_eq!(parsed.get(15).unwrap().room, "Cozinha");
        assert!(parsed.get(0).is_none());
    }
}
