# mc-tunnel - hGB (hidden Gate Bypass)

Workspace consisting of three crates:

- `tunnel-proto` — shared protocol implementation: VarInt/MC packets, HKDF key derivation, AES-256-GCM chunk encryption, and relay functions.
- `tunnel-server` — fake MC frontend + config with client access list.
- `tunnel-client` — local SOCKS5 proxy + config with server address and token.

## Building

```bash
cargo build --release
```

The binaries will be located at `target/release/tunnel-server` and `target/release/tunnel-client`.

Built and tested on `rustc`/`cargo` 1.75 (2021 edition), so it should compile smoothly on other systems as well.

## Cross-compilation (Windows, other Linux distros, ARM)

The cryptographic stack (`aes-gcm`, `hkdf`, `sha2`) relies on pure Rust implementations without C dependencies like OpenSSL. Therefore, cross-compilation does not run into system library linking issues and works seamlessly out of the box.

The easiest way to build for other targets is using [`cross`](https://github.com/cross-rs/cross) — it runs the build inside Docker containers pre-configured with the required toolchains and linkers.

```bash
cargo install cross --git https://github.com/cross-rs/cross
./scripts/build-all.sh
```

The script builds binaries for four target architectures and places them in `dist/`:

- `x86_64-unknown-linux-musl` — standard Linux x86_64 (Debian/Ubuntu, etc.), static binary with no dependency on the target glibc version.
- `aarch64-unknown-linux-musl` — ARM64 (RPi 4/5 on 64-bit OS, ARM VPS).
- `armv7-unknown-linux-musleabihf` — ARM32 (RPi 3B and similar 32-bit SBCs).
- `x86_64-pc-windows-gnu` — Windows 64-bit.

If you only need a single target, you can compile it directly without running the script:

```bash
cross build --release --target aarch64-unknown-linux-musl -p tunnel-server -p tunnel-client
```

The compiled binary will be placed in `target/<target>/release/`.

## Configuration

Both configurations use standard YAML format. Example configuration files are located alongside the source crates:

- `tunnel-server/config.example.yaml`
- `tunnel-client/config.example.yaml`

Rename them to `config.yaml` (or any custom name — you can specify the path using the `--config` flag) and set your parameters.

### Tunnel secret

`tunnel_secret_hex` — shared 32-byte hex-encoded key that **must match** on the server and all connected clients (HKDF derives separate keys for c2s/s2c streams using a per-connection salt). Generate one via:

```bash
openssl rand -hex 32
```

### Client list (server)

```yaml
clients:
  - uuid: "11111111-2222-3333-4444-555555555555"
    note: "first-phone"
    enabled: true
  - uuid: "22222222-3333-4444-5555-666666666666"
    note: "first-laptop"
    enabled: false   # temporarily disabled client
```

A client is identified by a UUID, which is sent during the `Login Start` handshake instead of a standard Mojang UUID.
Setting `enabled: false` retains the client entry in the configuration while blocking connection attempts.

### Client token

In `tunnel-client/config.yaml`, set the `token` field to match the UUID assigned to this device in the server's `clients` list.

## Running

```bash
./tunnel-server --config /etc/mc-tunnel/server.yaml

./tunnel-client --config ~/.config/mc-tunnel/client.yaml
```

By default, the client starts a SOCKS5 proxy bound to `127.0.0.1:1080`.

### Logging

By default (without setting `RUST_LOG`), the application outputs basic info log messages.
For detailed debugging output, set `RUST_LOG=debug`. To minimize logging output, set `RUST_LOG=warn`:

```bash
RUST_LOG=debug ./tunnel-server --config server.yaml
RUST_LOG=warn  ./tunnel-client --config client.yaml
```