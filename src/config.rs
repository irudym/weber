use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ManagedProcConfig {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub dir: Option<String>,
    pub stop_timeout_secs: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    process: Vec<ManagedProcConfig>,
}

// Borrows the list instead of cloning it
#[derive(Serialize)]
struct ConfigFileRef<'a> {
    process: &'a [ManagedProcConfig],
}

pub async fn save_configs(configs: &[ManagedProcConfig]) -> std::io::Result<()> {
    let toml =
        toml::to_string(&ConfigFileRef { process: configs }).map_err(std::io::Error::other)?;

    tokio::fs::write("processes.toml.tmp", toml).await?;
    tokio::fs::rename("processes.toml.tmp", "processes.toml").await
}

pub async fn load_configs() -> std::io::Result<Vec<ManagedProcConfig>> {
    let str = tokio::fs::read_to_string("processes.toml").await?;
    let file: ConfigFile = toml::from_str(&str).map_err(std::io::Error::other)?;
    Ok(file.process)
}

pub fn remove_from_config(configs: &mut Vec<ManagedProcConfig>, name: &str) -> bool {
    let len = configs.len();
    configs.retain(|p| p.name != name);
    len != configs.len()
}
