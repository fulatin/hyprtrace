#!/bin/bash
set -e

echo "=== HyprTrace Uninstall Script ==="

DO_REMOVE_DATA=false
DO_REMOVE_CONFIG=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)
      DO_REMOVE_DATA=true
      DO_REMOVE_CONFIG=true
      shift
      ;;
    --data)
      DO_REMOVE_DATA=true
      shift
      ;;
    --config)
      DO_REMOVE_CONFIG=true
      shift
      ;;
    *)
      echo "Usage: $0 [--all | --data | --config]"
      echo "  --all     Remove everything including data and config"
      echo "  --data    Also remove database and AI conversation history"
      echo "  --config  Also remove config file"
      exit 1
      ;;
  esac
done

echo "Stopping and disabling systemd services..."
systemctl --user stop hyprtrace-daemon.service 2>/dev/null || true
systemctl --user stop hyprtrace-server.service 2>/dev/null || true
systemctl --user disable hyprtrace-daemon.service 2>/dev/null || true
systemctl --user disable hyprtrace-server.service 2>/dev/null || true

echo "Removing systemd service files..."
rm -f ~/.config/systemd/user/hyprtrace-daemon.service
rm -f ~/.config/systemd/user/hyprtrace-server.service
systemctl --user daemon-reload

echo "Removing binaries..."
rm -f ~/.local/bin/hyprtrace-daemon
rm -f ~/.local/bin/hyprtrace-server

echo "Removing web assets..."
rm -rf ~/.local/share/hyprtrace/web

if [ "$DO_REMOVE_DATA" = true ]; then
  echo "Removing database..."
  rm -f ~/.local/share/hyprtrace/hyprtrace.db
  rm -f ~/.local/share/hyprtrace/hyprtrace.db-wal
  rm -f ~/.local/share/hyprtrace/hyprtrace.db-shm
  rmdir ~/.local/share/hyprtrace 2>/dev/null || true
fi

if [ "$DO_REMOVE_CONFIG" = true ]; then
  echo "Removing config..."
  rm -f ~/.config/hyprtrace/config.toml
  rmdir ~/.config/hyprtrace 2>/dev/null || true
fi

echo ""
echo "HyprTrace uninstalled successfully."
