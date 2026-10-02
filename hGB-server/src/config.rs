use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientEntry {
    pub uuid: Uuid,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FakeOnlineConfig {
    #[serde(default = "default_baseline")]
    pub baseline: u32,
    #[serde(default = "default_bounds")]
    pub bounds: (u32, u32),
    #[serde(default = "default_interval")]
    pub walk_interval_secs: (u64, u64),
}

fn default_baseline() -> u32 {
    2
}
fn default_bounds() -> (u32, u32) {
    (0, 6)
}
fn default_interval() -> (u64, u64) {
    (120, 400)
}

impl Default for FakeOnlineConfig {
    fn default() -> Self {
        Self {
            baseline: default_baseline(),
            bounds: default_bounds(),
            walk_interval_secs: default_interval(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen_host")]
    pub listen_host: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,

    pub tunnel_secret_hex: String,

    #[serde(default = "default_protocol")]
    pub protocol_version: i32,
    #[serde(default = "default_version_name")]
    pub version_name: String,

    pub motd: String,
    #[serde(default = "default_max_players")]
    pub max_players_display: u32,
    #[serde(default)]
    pub fake_names: Vec<String>,
    #[serde(default)]
    pub fake_online: FakeOnlineConfig,

    #[serde(default = "default_disconnect_message")]
    pub disconnect_message: String,

    pub clients: Vec<ClientEntry>,
}



fn default_listen_host() -> String {
    "0.0.0.0".to_string()
}
fn default_listen_port() -> u16 {
    25565
}
fn default_protocol() -> i32 {
    765
}
fn default_version_name() -> String {
    "1.20.4".to_string()
}
fn default_max_players() -> u32 {
    67
}
fn default_disconnect_message() -> String {
    "§fYou are not whitelisted on this server!".to_string()
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
        if cfg.fake_names.is_empty() {
            bail!("fake_names should not be empty (need names for players.sample)");
        }

        Ok(cfg)
    }

    pub fn tunnel_secret(&self) -> Vec<u8> {
        hex::decode(self.tunnel_secret_hex.trim()).expect("already valided")
    }

    pub fn enabled_clients(&self) -> HashMap<Uuid, ClientEntry> {
        self.clients
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.uuid, c.clone()))
            .collect()
    }
}
