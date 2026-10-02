mod config;

use anyhow::Result;
use clap::Parser;
use config::Config;
use rand::RngCore;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use hGB_proto::{
    derive_key, relay_encrypted_to_plain, relay_plain_to_encrypted, write_chunk, write_string,
    write_varint, NonceCounter, SALT_LEN,
};
use uuid::Uuid;

struct AppState {
    cfg: Config,
    active: AtomicUsize,
}

#[derive(Parser, Debug)]
#[command(about = "A local SOCKS5 proxy over a tunnel disguised as Minecraft")]
struct Args {
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,
}

fn build_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = write_varint(packet_id);
    body.extend_from_slice(payload);
    let mut out = write_varint(body.len() as i32);
    out.extend_from_slice(&body);
    out
}

fn build_handshake(protocol: i32, address: &str, port: u16, next_state: i32) -> Vec<u8> {
    let mut payload = write_varint(protocol);
    payload.extend_from_slice(&write_string(address));
    payload.extend_from_slice(&port.to_be_bytes());
    payload.extend_from_slice(&write_varint(next_state));
    build_packet(0x00, &payload)
}

fn build_login_start(name: &str, player_uuid: &Uuid) -> Vec<u8> {
    let mut payload = write_string(name);
    payload.extend_from_slice(player_uuid.as_bytes());
    build_packet(0x00, &payload)
}

async fn open_tunnel(
    cfg: &Config,
    dest_host: &str,
    dest_port: u16,
) -> Result<(TcpStream, [u8; 32], [u8; 32], NonceCounter)> {
    let mut remote = TcpStream::connect((cfg.remote_host.as_str(), cfg.remote_port)).await?;
    remote.set_nodelay(true).ok();

    let handshake = build_handshake(
        cfg.protocol_version,
        &cfg.fake_server_address,
        cfg.remote_port,
        2,
    );
    remote.write_all(&handshake).await?;

    let fake_name = format!("Player{:06x}", rand::thread_rng().next_u32() & 0xFFFFFF);
    let login = build_login_start(&fake_name, &cfg.token);
    remote.write_all(&login).await?;
    remote.flush().await?;

    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    let secret = cfg.tunnel_secret();
    let c2s_key = derive_key(&secret, &salt, b"c2s");
    let s2c_key = derive_key(&secret, &salt, b"s2c");

    remote.write_all(&salt).await?;
    remote.flush().await?;

    let dest = format!("{dest_host}:{dest_port}");
    let mut send_nonces = NonceCounter::new();
    write_chunk(&mut remote, &c2s_key, &mut send_nonces, dest.as_bytes()).await?;

    Ok((remote, c2s_key, s2c_key, send_nonces))
}

const SOCKS_VERSION: u8 = 5;

async fn socks5_handshake<S: AsyncReadExt + AsyncWriteExt + Unpin>(stream: &mut S) -> Result<bool> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await?;
    let (ver, nmethods) = (header[0], header[1]);
    if ver != SOCKS_VERSION {
        return Ok(false);
    }
    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;
    stream.write_all(&[SOCKS_VERSION, 0x00]).await?;
    stream.flush().await?;
    Ok(true)
}

enum SocksAddr {
    V4(Ipv4Addr),
    Domain(String),
    V6(Ipv6Addr),
}

async fn socks5_read_request<S: AsyncReadExt + Unpin>(
    stream: &mut S,
) -> Result<Option<(u8, SocksAddr, u16)>> {
    let mut req = [0u8; 4];
    stream.read_exact(&mut req).await?;
    let (_ver, cmd, _rsv, atyp) = (req[0], req[1], req[2], req[3]);

    let addr = match atyp {
        1 => {
            let mut b = [0u8; 4];
            stream.read_exact(&mut b).await?;
            SocksAddr::V4(Ipv4Addr::from(b))
        }
        3 => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let mut buf = vec![0u8; len_buf[0] as usize];
            stream.read_exact(&mut buf).await?;
            SocksAddr::Domain(String::from_utf8(buf)?)
        }
        4 => {
            let mut b = [0u8; 16];
            stream.read_exact(&mut b).await?;
            SocksAddr::V6(Ipv6Addr::from(b))
        }
        _ => return Ok(None),
    };

    let mut port_buf = [0u8; 2];
    stream.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);

    Ok(Some((cmd, addr, port)))
}

fn socks5_reply(rep: u8) -> [u8; 10] {
    let mut out = [0u8; 10];
    out[0] = SOCKS_VERSION;
    out[1] = rep;
    out[2] = 0x00;
    out[3] = 0x01;
    out
}

async fn handle_local_client(mut local: TcpStream, state: Arc<AppState>) -> Result<()> {
    let peer = local.peer_addr().ok();
    let peer_label = peer.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());

    if !socks5_handshake(&mut local).await? {
        return Ok(());
    }

    let req = socks5_read_request(&mut local).await?;
    let (cmd, addr, port) = match req {
        Some(v) => v,
        None => {
            local.write_all(&socks5_reply(0x08)).await?;
            return Ok(());
        }
    };

    if cmd != 1 {
        local.write_all(&socks5_reply(0x07)).await?;
        return Ok(());
    }

    let dest_host = match &addr {
        SocksAddr::V4(ip) => ip.to_string(),
        SocksAddr::Domain(d) => d.clone(),
        SocksAddr::V6(ip) => ip.to_string(),
    };

    let (remote, c2s_key, s2c_key, send_nonces) =
        match open_tunnel(&state.cfg, &dest_host, port).await {
            Ok(v) => v,
            Err(e) => {
                warn!("✗ {peer_label}: Raising the tunnel to the {dest_host}:{port} level was not feasible - {e}");
                local.write_all(&socks5_reply(0x01)).await?;
                return Ok(());
            }
        };

    local.write_all(&socks5_reply(0x00)).await?;
    local.flush().await?;

    let active = state.active.fetch_add(1, Ordering::Relaxed) + 1;
    info!("✅ {peer_label}: The tunnel is active -> {dest_host}:{port} - active sessions: {active}");

    let (remote_reader, remote_writer) = remote.into_split();
    let (local_reader, local_writer) = local.into_split();

    let result = tokio::try_join!(
        relay_plain_to_encrypted(local_reader, remote_writer, c2s_key, send_nonces),
        relay_encrypted_to_plain(remote_reader, s2c_key, NonceCounter::new(), local_writer),
    );

    let active = state.active.fetch_sub(1, Ordering::Relaxed) - 1;
    info!("❌ {peer_label}: tunnel to {dest_host}:{port} closed - active sessions: {active}");

    result?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    let cfg = Config::load(&args.config)?;

    let listener = TcpListener::bind((cfg.local_host.as_str(), cfg.local_port)).await?;

    info!("========================================");
    info!("  mc-tunnel client started");
    info!("  local SOCKS5: {}:{}", cfg.local_host, cfg.local_port);
    info!("  tunnel to: {}:{}", cfg.remote_host, cfg.remote_port);
    info!("  token: {}", cfg.token);
    info!("========================================");

    let state = Arc::new(AppState {
        cfg,
        active: AtomicUsize::new(0),
    });

    loop {
        let (stream, peer) = listener.accept().await?;
        stream.set_nodelay(true).ok();
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_local_client(stream, state).await {
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

