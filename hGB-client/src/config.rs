use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_local_host")]
    pub local_host: String,
    #[serde(default = "default_local_port")]
    pub local_port: u16,

    pub remote_host: String,
    #[serde(default = "default_remote_port")]
    pub remote_port: u16,
    #[serde(default = "default_fake_server_address")]
    pub fake_server_address: String,

    #[serde(default = "default_protocol")]
    pub protocol_version: i32,
    pub tunnel_secret_hex: String,
    pub token: Uuid,
}

fn default_local_host() -> String {
    "127.0.0.1".to_string()
}
fn default_local_port() -> u16 {
    1080
}
fn default_remote_port() -> u16 {
    25565
}
fn default_fake_server_address() -> String {
    "mc.example.net".to_string()
}
fn default_protocol() -> i32 {
    765
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("couldn't to read config {}", path.display()))?;
        let cfg: Config = serde_yaml::from_str(&raw)
            .with_context(|| format!("couldn't to parse config {}", path.display()))?;

        let secret = hex::decode(cfg.tunnel_secret_hex.trim())
            .context("tunnel_secret_hex must to be valid hex")?;
        if secret.len() != 32 {
            bail!(
                "tunnel_secret_hex must decode in 32 bytes, but {}",
                secret.len()
            );
        }

        Ok(cfg)
    }

    pub fn tunnel_secret(&self) -> Vec<u8> {
        hex::decode(self.tunnel_secret_hex.trim()).expect("already valided")
    }
}
