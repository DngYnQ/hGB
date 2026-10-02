set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS=(
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
  "armv7-unknown-linux-musleabihf"
  "x86_64-pc-windows-gnu"
)

mkdir -p dist

for target in "${TARGETS[@]}"; do
  echo "=== Сборка под $target ==="
  cross build --release --target "$target" -p hGB-server -p hGB-client

  out="dist/$target"
  mkdir -p "$out"

  if [[ "$target" == *windows* ]]; then
    cp "target/$target/release/hGB-server.exe" "$out/"
    cp "target/$target/release/hGB-client.exe" "$out/"
  else
    cp "target/$target/release/hGB-server" "$out/"
    cp "target/$target/release/hGB-client" "$out/"
  fi

  cp hGB-server/config.example.yaml "$out/server.config.example.yaml"
  cp hGB-client/config.example.yaml "$out/client.config.example.yaml"

  echo "  -> $out"
done

echo
echo "Done. Binares in dist/:"
find dist -type f | sort
