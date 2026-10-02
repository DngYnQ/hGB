# mc-tunnel - hGB(hidden Gate Bypass)

Воркспейс из трёх крейтов:

- `tunnel-proto` - общий протокол: VarInt/пакеты MC, HKDF-деривация ключей,
  AES-256-GCM чанки, релей-функции.
- `tunnel-server` - фейковый MC-фронтенд + конфиг со списком клиентов.
- `tunnel-client` - локальный SOCKS5 + конфиг с адресом сервера и токеном.

## Сборка

```bash
cargo build --release
```

Бинарники окажутся в `target/release/tunnel-server` и
`target/release/tunnel-client`.

Билдил и тестил на `rustc`/`cargo` 1.75 (edition 2021), так что
должно без проблем собираться и на других.

## Сборка под другие платформы (Windows, другой Linux, ARM)

Криптографически (`aes-gcm`, `hkdf`, `sha2`) - Rust без OpenSSL и других
зависимостей, кросс-компиляция не упирается в линковку
системных библиотек и работает без лишних бубнов.

Собрать проще через [`cross`](https://github.com/cross-rs/cross) - он гоняет
сборку в Docker-контейнере с нужным тулчейном и линкерами.

```bash
cargo install cross --git https://github.com/cross-rs/cross
./scripts/build-all.sh
```

Скрипт соберёт под четыре бинарника и разложит их по `dist/`:

- `x86_64-unknown-linux-musl` - обычный Linux x86_64 (Debian/Ubuntu/и т.д.),
  статический бинарник без привязки к версии glibc дистрибутива
- `aarch64-unknown-linux-musl` - ARM64 (RPi4/5 на 64-битной ОС, ARM-VPS)
- `armv7-unknown-linux-musleabihf` - ARM32 (RPi3B и подобные 32-битные SBC)
- `x86_64-pc-windows-gnu` - Windows 64-бит

Если нужен только один бинарник - то же самое без скрипта:

```bash
cross build --release --target aarch64-unknown-linux-musl -p tunnel-server -p tunnel-client
```

Бинарник появится в `target/<таргет>/release/`.

## Конфиги

Оба конфига - YAML, примеры лежат рядом с исходниками:

- `tunnel-server/config.example.yaml`
- `tunnel-client/config.example.yaml`

Скопируй их в `config.yaml` (или любое другое имя - путь передаётся через
`--config`) и подставь свои значения.

### Секрет тоннеля

`tunnel_secret_hex` - общий 32-байтный ключ в hex, должен **совпадать**
на сервере и у всех клиентов (им HKDF выводит разные ключи на c2s/s2c по
соли конкретного соединения). Сгенерировать:

```bash
openssl rand -hex 32
```

### Список клиентов (сервер)

```yaml
clients:
  - uuid: "11111111-2222-3333-4444-555555555555"
    note: "first-phone"
    enabled: true
  - uuid: "22222222-3333-4444-5555-666666666666"
    note: "first-laptop"
    enabled: false   # временно отключённый клиент
```

Клиент - это UUID, который отправляет сам в Login Start вместо обычного
UUID аккаунта Mojang. `enabled: false` держит запись в конфиге, но не
пускает в тоннель.

### Токен клиента

В `tunnel-client/config.yaml` поле `token` - тот же UUID, что в списке
`clients` сервера для этого устройства.

## Запуск

```bash
# на сервере
./tunnel-server --config /etc/mc-tunnel/server.yaml

# на клиентской машине
./tunnel-client --config ~/.config/mc-tunnel/client.yaml
```

По умолчанию клиент поднимает SOCKS5 на `127.0.0.1:1080`

По умолчанию (без `RUST_LOG`) в консоль выводиться основные данные.
Если нужно больше деталей -
`RUST_LOG=debug`, если наоборот тише - `RUST_LOG=warn`:

```bash
RUST_LOG=debug ./tunnel-server --config server.yaml
RUST_LOG=warn  ./tunnel-client --config client.yaml
```
