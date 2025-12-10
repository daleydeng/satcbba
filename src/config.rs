use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use std::path::Path;
use std::process::Command;
use tracing::error;

pub fn load_pkl<T: DeserializeOwned>(config_path: impl AsRef<Path>) -> Result<T> {
    let config_path = config_path.as_ref();
    let output = Command::new("pkl")
        .arg("eval")
        .arg("-f")
        .arg("json")
        .arg(config_path)
        .output()
        .context("Failed to execute pkl command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("pkl failed: {}", stderr);
        return Err(anyhow!("pkl failed: {}", stderr));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    let config: T = serde_json::from_str(&json_str).context("Failed to parse config json")?;

    Ok(config)
}
