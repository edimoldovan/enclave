#!/usr/bin/env bash
# Installs Enclave for the current user (binary, icon and desktop entry).
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="${PREFIX:-$HOME/.local}"

echo "Building release binary…"
cargo build --release --manifest-path "$repo/Cargo.toml"

install -Dm755 "$repo/target/release/enclave" "$prefix/bin/enclave"
install -Dm644 "$repo/packaging/enclave.svg" \
  "$prefix/share/icons/hicolor/scalable/apps/enclave.svg"
install -Dm644 "$repo/packaging/enclave.desktop" \
  "$prefix/share/applications/enclave.desktop"
install -Dm644 "$repo/crates/grido/keymap.toml" "$HOME/.config/grido/keymap.toml"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$prefix/share/applications" || true
fi

echo "Installed to $prefix/bin/enclave"
echo "Keymap installed at ~/.config/grido/keymap.toml — edit to taste."

# Nothing to do for assistants: Enclave registers itself with any MCP clients
# it finds the first time it runs.
