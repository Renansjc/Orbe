use std::{env, fs, io, path::PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalState {
    pub pending_commissioning: bool,
}

pub fn state_path() -> PathBuf {
    data_dir().join("state.txt")
}

/// Pasta de dados local: `ORBE_DATA_DIR`, senão `%USERPROFILE%\.orbe`.
/// Fica fora do AppData de propósito: apps empacotados (MSIX) têm o AppData
/// virtualizado e enxergariam um estado diferente.
fn data_dir() -> PathBuf {
    env::var_os("ORBE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".orbe")))
        .unwrap_or_else(|| env::temp_dir().join(".orbe"))
}

pub fn app_data_dir() -> PathBuf {
    state_path()
        .parent()
        .expect("state path always has a parent")
        .to_owned()
}

pub fn controller_store_path() -> PathBuf {
    app_data_dir().join("matter-controller-state.bin")
}

pub fn paa_roots_dir() -> PathBuf {
    app_data_dir().join("trust").join("paa")
}

pub fn cd_roots_dir() -> PathBuf {
    app_data_dir().join("trust").join("cd")
}

pub fn load() -> io::Result<LocalState> {
    let path = state_path();
    match fs::read_to_string(path) {
        Ok(contents) => Ok(LocalState {
            pending_commissioning: contents.trim() == "pending_commissioning=true",
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LocalState::default()),
        Err(error) => Err(error),
    }
}

pub fn save(state: &LocalState) -> io::Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("pending_commissioning={}\n", state.pending_commissioning),
    )
}

