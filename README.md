# mc-tunnel - hGB (hidden Gate Bypass)

Rust-based client and server implementing a SOCKS5 tunnel over the Minecraft protocol. Simple and easy to set up

[RU](docs/ru.md)
[EU](docs/eu.md)

## Configuration

Both configurations use standard YAML format. Example configuration files are located alongside the source crates:

- `tunnel-server/config.example.yaml`
- `tunnel-client/config.example.yaml`

Rename them to `config.yaml` (or any custom name — you can specify the path using the `--config` flag) and set your parameters.


## Running

```bash
./tunnel-server --config /etc/mc-tunnel/server.yaml

./tunnel-client --config ~/.config/mc-tunnel/client.yaml
```

By default, the client starts a SOCKS5 proxy bound to `127.0.0.1:1080`.
