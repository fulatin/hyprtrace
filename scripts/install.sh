#!/bin/bash
set -e

echo "=== HyprTrace Install Script ==="

command -v cargo >/dev/null 2>&1 || { echo "Error: Rust toolchain (cargo) is required"; exit 1; }
command -v node >/dev/null 2>&1 || { echo "Error: Node.js is required"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "Error: npm is required"; exit 1; }

echo "Building Rust components..."
cargo build --release
cp target/release/hyprtrace-daemon ~/.local/bin/
cp target/release/hyprtrace-server ~/.local/bin/

echo "Building frontend..."
cd web
npm install
npm run build
mkdir -p ~/.local/share/hyprtrace/web
cp -r dist/* ~/.local/share/hyprtrace/web/
cd ..

echo "Installing systemd services..."
mkdir -p ~/.config/systemd/user/
cp scripts/hyprtrace-daemon.service ~/.config/systemd/user/
cp scripts/hyprtrace-server.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now hyprtrace-daemon.service
systemctl --user enable --now hyprtrace-server.service

echo ""
echo "HyprTrace installed successfully!"
echo "  Frontend: http://localhost:9420"
echo "  Database: ~/.local/share/hyprtrace/hyprtrace.db"
echo "  Config:   ~/.config/hyprtrace/config.toml"