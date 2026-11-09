use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ManagedProcConfig {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub dir: Option<String>,
    pub stop_timeout_secs: u64,
}

pub async fn save_configs(configs: &[ManagedProcConfig]) -> std::io::Result<()> {
    let toml = toml::to_string(configs)?;
    tokio::fs::write("processes.toml.tmp", toml).await?;
    tokio::fs::rename("processes.toml.tmp", "processes.toml").await
}
