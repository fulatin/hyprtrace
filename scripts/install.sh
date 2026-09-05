#!/bin/bash
set -e

# Every path below ("web/", "scripts/", the cargo workspace) is relative to the
# repository root, so anchor there instead of depending on where the script was
# invoked from.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=== HyprTrace Install Script ==="

command -v cargo >/dev/null 2>&1 || { echo "Error: Rust toolchain (cargo) is required"; exit 1; }
command -v node >/dev/null 2>&1 || { echo "Error: Node.js is required"; exit 1; }
command -v npm >/dev/null 2>&1 || { echo "Error: npm is required"; exit 1; }

# The account that will actually run the daemon. Under `sudo install.sh` that
# is the invoking user, not root: checking `id -nG` alone reports root's
# groups, so the check below would add *root* to the input group and leave the
# real user without device access. Files are installed to that user's home too,
# because `~` expands to root's home under sudo.
RUNNING_AS_ROOT=0
if [ "$(id -u)" = "0" ] && [ -n "$SUDO_USER" ]; then
    TARGET_USER="$SUDO_USER"
    RUNNING_AS_ROOT=1
else
    TARGET_USER="$(id -un)"
fi
TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
if [ -z "$TARGET_HOME" ]; then
    echo "Error: cannot determine the home directory of $TARGET_USER"
    exit 1
fi

# Take ownership back when the build ran as root.
fix_owner() {
    [ "$RUNNING_AS_ROOT" = "1" ] && chown -R "$TARGET_USER":"$(id -gn "$TARGET_USER")" "$1"
    return 0
}

# Input activity monitoring reads /dev/input/event* (keyboard/mouse). It is
# optional: without it the idle fallback only notices window switches.
if ! id -nG "$TARGET_USER" | tr ' ' '\n' | grep -qx input; then
    echo "Note: $TARGET_USER is not in the 'input' group. Keyboard/mouse activity"
    echo "detection (accurate idle tracking without loginctl) will be disabled."
    if [ "$RUNNING_AS_ROOT" = "1" ]; then
        usermod -aG input "$TARGET_USER"
        echo "Added $TARGET_USER to the input group (re-login required)."
    else
        echo "Run this to enable it: sudo usermod -aG input \$USER  (then re-login)"
    fi
fi

echo "Building Rust components..."
cargo build --release
# ~/.local/bin is not guaranteed to exist; without this the cp below fails.
mkdir -p "$TARGET_HOME/.local/bin"
cp target/release/hyprtrace-daemon "$TARGET_HOME/.local/bin/"
cp target/release/hyprtrace-server "$TARGET_HOME/.local/bin/"
fix_owner "$TARGET_HOME/.local/bin"

echo "Building frontend..."
cd web
# A lockfile is committed, so install exactly what it pins — `npm install` can
# silently upgrade transitive dependencies and produce a build that differs
# from the one that was reviewed.
if [ -f package-lock.json ]; then
    npm ci
else
    npm install
fi
npm run build
mkdir -p "$TARGET_HOME/.local/share/hyprtrace/web"
# `dist/.` rather than `dist/*`: the glob fails when dist is empty and skips
# dotfiles.
cp -r dist/. "$TARGET_HOME/.local/share/hyprtrace/web/"
fix_owner "$TARGET_HOME/.local/share/hyprtrace"
cd ..

if [ "$RUNNING_AS_ROOT" = "1" ]; then
    # `systemctl --user` talks to the per-user D-Bus session, which does not
    # exist for root's environment here. Enabling the units as the target user
    # from this context is unreliable, so install the files and print the
    # commands instead of reporting a success that did not happen.
    echo "Installing systemd services for $TARGET_USER..."
    mkdir -p "$TARGET_HOME/.config/systemd/user/"
    cp scripts/hyprtrace-daemon.service "$TARGET_HOME/.config/systemd/user/"
    cp scripts/hyprtrace-server.service "$TARGET_HOME/.config/systemd/user/"
    fix_owner "$TARGET_HOME/.config/systemd/user"
    echo ""
    echo "Service files installed. Finish the activation in your own session:"
    echo "  systemctl --user daemon-reload"
    echo "  systemctl --user enable --now hyprtrace-daemon.service"
    echo "  systemctl --user enable --now hyprtrace-server.service"
else
    echo "Installing systemd services..."
    mkdir -p ~/.config/systemd/user/
    cp scripts/hyprtrace-daemon.service ~/.config/systemd/user/
    cp scripts/hyprtrace-server.service ~/.config/systemd/user/
    systemctl --user daemon-reload
    systemctl --user enable --now hyprtrace-daemon.service
    systemctl --user enable --now hyprtrace-server.service
fi

echo ""
echo "HyprTrace installed successfully!"
echo "  Frontend: http://localhost:9420"
echo "  Database: ~/.local/share/hyprtrace/hyprtrace.db"
echo "  Config:   ~/.config/hyprtrace/config.toml"
