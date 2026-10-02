mod config;

use anyhow::Result;
use clap::Parser;
use config::{ClientEntry, Config};
use rand::seq::SliceRandom;
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use hGB_proto::{
    derive_key, read_chunk, read_packet, read_string_from_bytes, read_varint_from_bytes,
    relay_encrypted_to_plain, relay_plain_to_encrypted, send_packet, NonceCounter, SALT_LEN,
};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(about = "Fake Minecraft-frontend over AEAD-tunnel")]
struct Args {
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,
}

struct SharedState {
    cfg: Config,
    secret: Vec<u8>,
    clients: HashMap<Uuid, ClientEntry>,
    baseline: AtomicU32,
    active: AtomicUsize,
}

impl SharedState {
    fn current_online(&self) -> u32 {
        let baseline = self.baseline.load(Ordering::Relaxed);
        baseline + (self.active.load(Ordering::Relaxed) as u32 / 5)
    }

    fn sample_players(&self, count: u32) -> Vec<serde_json::Value> {
        let mut rng = rand::thread_rng();
        let n = (count as usize).min(self.cfg.fake_names.len());
        let mut names = self.cfg.fake_names.clone();
        names.shuffle(&mut rng);
        names
            .into_iter()
            .take(n)
            .map(|name| json!({ "name": name, "id": Uuid::new_v4().to_string() }))
            .collect()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    let cfg = Config::load(&args.config)?;
    let secret = cfg.tunnel_secret();
    let clients = cfg.enabled_clients();

    let state = Arc::new(SharedState {
        baseline: AtomicU32::new(cfg.fake_online.baseline),
        active: AtomicUsize::new(0),
        secret,
        clients,
        cfg,
    });

    tokio::spawn(fake_baseline_updater(state.clone()));

    let listen_addr = format!("{}:{}", state.cfg.listen_host, state.cfg.listen_port);
    let listener = TcpListener::bind(&listen_addr).await?;

    info!("========================================");
    info!("  mc-tunnel server started");
    info!("  listen: {listen_addr}");
    info!("  MOTD: {}", state.cfg.motd);
    info!("  clients in config: {}", state.clients.len());
    for c in state.clients.values() {
        info!("    - {} ({})", c.note, c.uuid);
    }
    info!("========================================");

    loop {
        let (stream, peer) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, peer, state).await {
                let benign = e
                    .downcast_ref::<hGB_proto::TunnelError>()
                    .map(|te| te.is_benign_close())
                    .unwrap_or(false);
                if !benign {
                    warn!("{peer}: err with connect: {e}");
                }
            }
        });
    }
}

async fn fake_baseline_updater(state: Arc<SharedState>) {
    let (lo, hi) = state.cfg.fake_online.walk_interval_secs;
    let (min_b, max_b) = state.cfg.fake_online.bounds;
    loop {
        let secs = rand::thread_rng().gen_range(lo..=hi.max(lo));
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
        let delta: i32 = *[-1, -1, 0, 0, 1, 1].choose(&mut rand::thread_rng()).unwrap();
        let cur = state.baseline.load(Ordering::Relaxed) as i32;
        let new_val = (cur + delta).clamp(min_b as i32, max_b as i32) as u32;
        state.baseline.store(new_val, Ordering::Relaxed);
    }
}

async fn handle_client(stream: TcpStream, peer: SocketAddr, state: Arc<SharedState>) -> Result<()> {
    stream.set_nodelay(true).ok();
    let (mut reader, mut writer) = stream.into_split();

    let (pid, payload) = read_packet(&mut reader).await?;
    if pid != 0x00 {
        return Ok(());
    }

    let (_protocol_version, off) = read_varint_from_bytes(&payload)?;
    let (_server_addr, off2) = read_string_from_bytes(&payload[off..])?;
    let off = off + off2;
    let _server_port = u16::from_be_bytes([payload[off], payload[off + 1]]);
    let off = off + 2;
    let (next_state, _) = read_varint_from_bytes(&payload[off..])?;

    match next_state {
        1 => handle_status_state(&mut reader, &mut writer, &state).await?,
        2 => handle_login_state(reader, writer, peer, &state).await?,
        _ => {}
    }

    Ok(())
}

async fn handle_status_state(
    reader: &mut OwnedReadHalf,
    writer: &mut OwnedWriteHalf,
    state: &Arc<SharedState>,
) -> Result<()> {
    loop {
        let (pid, payload) = read_packet(reader).await?;
        match pid {
            0x00 => {
                let online = state.current_online();
                let response = json!({
                    "version": { "name": state.cfg.version_name, "protocol": state.cfg.protocol_version },
                    "players": {
                        "max": state.cfg.max_players_display,
                        "online": online,
                        "sample": state.sample_players(online),
                    },
                    "description": { "text": state.cfg.motd },
                });
                let body = hGB_proto::write_string(&response.to_string());
                send_packet(writer, 0x00, &body).await?;
            }
            0x01 => {
                send_packet(writer, 0x01, &payload).await?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

fn chat_component(text: &str) -> String {
    json!({ "text": text, "color": "yellow" }).to_string()
}

async fn send_login_disconnect(writer: &mut OwnedWriteHalf, reason: &str) -> Result<()> {
    let body = hGB_proto::write_string(&chat_component(reason));
    send_packet(writer, 0x00, &body).await?;
    Ok(())
}

async fn handle_login_state(
    mut reader: OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    peer: SocketAddr,
    state: &Arc<SharedState>,
) -> Result<()> {
    let (pid, payload) = read_packet(&mut reader).await?;
    if pid != 0x00 {
        return Ok(());
    }

    let (name, off) = read_string_from_bytes(&payload)?;
    let player_uuid = if payload.len() - off >= 16 {
        Some(Uuid::from_slice(&payload[off..off + 16])?)
    } else {
        None
    };

    let entry = player_uuid.and_then(|u| state.clients.get(&u).cloned());

    match entry {
        Some(client) => {
            info!("🔓 {peer}: authorized as '{}' ({})", client.note, client.uuid);
            if let Err(e) = tunnel_hook(reader, writer, peer, client.clone(), state.clone()).await
            {
                let benign = e
                    .downcast_ref::<hGB_proto::TunnelError>()
                    .map(|te| te.is_benign_close())
                    .unwrap_or(false);
                if !benign {
                    warn!("⚠ {peer} ('{}'): err with tunnel: {e}", client.note);
                }
            }
        }
        None => {
            warn!(
                "⛔ {peer}: KICKED - not in whitelist (real nickname: {name:?}, uuid: {player_uuid:?})"
            );
            send_login_disconnect(&mut writer, &state.cfg.disconnect_message).await?;
        }
    }

    Ok(())
}

async fn tunnel_hook(
    mut reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    peer: SocketAddr,
    client: ClientEntry,
    state: Arc<SharedState>,
) -> Result<()> {
    let active = state.active.fetch_add(1, Ordering::Relaxed) + 1;
    info!("✅ {peer}: client '{}' connected - active sessions: {active}", client.note);

    let result = tunnel_hook_inner(&mut reader, writer, peer, &client, &state).await;

    let active = state.active.fetch_sub(1, Ordering::Relaxed) - 1;
    info!("❌ {peer}: client '{}' disconnected - active sessions: {active}", client.note);

    result
}

async fn tunnel_hook_inner(
    reader: &mut OwnedReadHalf,
    mut writer: OwnedWriteHalf,
    peer: SocketAddr,
    client: &ClientEntry,
    state: &Arc<SharedState>,
) -> Result<()> {
    use tokio::io::AsyncReadExt;

    let mut salt = [0u8; SALT_LEN];
    reader.read_exact(&mut salt).await?;
    let c2s_key = derive_key(&state.secret, &salt, b"c2s");
    let s2c_key = derive_key(&state.secret, &salt, b"s2c");
    let mut recv_nonces = NonceCounter::new();
    let send_nonces = NonceCounter::new();
    let dest_raw = read_chunk(reader, &c2s_key, &mut recv_nonces).await?;
    let dest = String::from_utf8(dest_raw)?;
    let (host, port_str) = dest
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("incorrect dest: {dest}"))?;
    let port: u16 = port_str.parse()?;

    info!("↪ {peer} ('{}'): CONNECT {host}:{port}", client.note);

    let remote = match TcpStream::connect((host, port)).await {
        Ok(s) => s,
        Err(e) => {
            warn!("✗ {peer} ('{}'): failed to connect to {host}:{port} - {e}", client.note);
            let _ = writer.shutdown().await;
            return Ok(());
        }
    };
    remote.set_nodelay(true).ok();
    let (remote_reader, remote_writer) = remote.into_split();

    tokio::try_join!(
        relay_encrypted_to_plain(reader, c2s_key, recv_nonces, remote_writer),
        relay_plain_to_encrypted(remote_reader, writer, s2c_key, send_nonces),
    )?;

    Ok(())
}
