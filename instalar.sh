#!/usr/bin/env bash
# ==============================================================================
# Corvo installation script for Linux and macOS
# ==============================================================================
# Run directly with:
# curl -fsSL https://raw.githubusercontent.com/diegoleteliers10/corvo/main/instalar.sh | bash
# ==============================================================================

set -euo pipefail

GITHUB_USER="diegoleteliers10"
GITHUB_REPO="corvo"
APP_NAME="corvo"

echo "=== Starting $APP_NAME installation ==="

# ── 1. Detect OS and architecture ─────────────────────────────────────────────
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux)
        if [ "$ARCH" = "x86_64" ] || [ "$ARCH" = "amd64" ]; then
            TARGET="x86_64-unknown-linux-gnu"
        else
            echo "ERROR: Linux architecture '$ARCH' is not supported." >&2
            exit 1
        fi
        ;;
    darwin)
        if [ "$ARCH" = "x86_64" ]; then
            TARGET="x86_64-apple-darwin"
        elif [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
            TARGET="aarch64-apple-darwin"
        else
            echo "ERROR: macOS architecture '$ARCH' is not supported." >&2
            exit 1
        fi
        ;;
    *)
        echo "ERROR: OS '$OS' is not compatible with this script." >&2
        exit 1
        ;;
esac

echo "Detected platform: OS=$OS, Arch=$ARCH -> Target=$TARGET"

# ── 1.5. Prevent conflict with package manager installs ───────────────────────
FORCE_INSTALL="${CORVO_FORCE_INSTALL:-0}"

if [ "$OS" = "darwin" ]; then
    for caskroom in "/opt/homebrew/Caskroom/$APP_NAME" "/usr/local/Caskroom/$APP_NAME"; do
        if [ -d "$caskroom" ] && [ "$FORCE_INSTALL" != "1" ]; then
            echo "NOTICE: Corvo is already installed via Homebrew (found $caskroom)." >&2
            echo "        Run 'brew upgrade --cask corvo' to update instead." >&2
            echo "        Re-run with CORVO_FORCE_INSTALL=1 to install alongside it anyway." >&2
            exit 1
        fi
    done
else
    if command -v dpkg >/dev/null 2>&1 && dpkg -s "$APP_NAME" >/dev/null 2>&1 && [ "$FORCE_INSTALL" != "1" ]; then
        echo "NOTICE: Corvo is already installed via a .deb package." >&2
        echo "        Run 'sudo apt update && sudo apt upgrade $APP_NAME' to update instead." >&2
        echo "        Re-run with CORVO_FORCE_INSTALL=1 to install alongside it anyway." >&2
        exit 1
    fi
fi

# ── 2. Resolve directories ───────────────────────────────────────────────────
if [ "$OS" = "darwin" ]; then
    CONFIG_DIR="$HOME/Library/Application Support/corvo"
    DATA_DIR="$HOME/Library/Application Support/corvo"
    CACHE_DIR="$HOME/Library/Caches/corvo"
    if [ -w "/usr/local/bin" ]; then
        BIN_DIR="/usr/local/bin"
    else
        BIN_DIR="$HOME/.local/bin"
    fi
else
    CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/corvo"
    DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/corvo"
    CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/corvo"
    BIN_DIR="$HOME/.local/bin"
fi

mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$CACHE_DIR" "$BIN_DIR"

# ── 3. Query GitHub API for the latest release ────────────────────────────────
echo "Fetching latest version from GitHub..."
API_URL="https://api.github.com/repos/$GITHUB_USER/$GITHUB_REPO/releases/latest"
API_RESPONSE=$(curl -sSfL "$API_URL")

if command -v jq >/dev/null 2>&1; then
    LATEST_TAG=$(printf '%s' "$API_RESPONSE" | jq -r '.tag_name')
elif command -v python3 >/dev/null 2>&1; then
    LATEST_TAG=$(printf '%s' "$API_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'])")
elif command -v python >/dev/null 2>&1; then
    LATEST_TAG=$(printf '%s' "$API_RESPONSE" | python -c "import sys,json; print(json.load(sys.stdin)['tag_name'])")
else
    LATEST_TAG=$(printf '%s' "$API_RESPONSE" \
        | tr -d ' \t\r\n' \
        | grep -o '"tag_name":"[^"]*"' \
        | head -1 \
        | sed -E 's/"tag_name":"([^"]+)"/\1/')
fi

if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "null" ]; then
    echo "ERROR: Could not determine latest release version from GitHub." >&2
    exit 1
fi

echo "Latest version found: $LATEST_TAG"

# ── 4. Download and extract asset ─────────────────────────────────────────────
ASSET_NAME="$APP_NAME-$TARGET.tar.gz"
DOWNLOAD_URL="https://github.com/$GITHUB_USER/$GITHUB_REPO/releases/download/$LATEST_TAG/$ASSET_NAME"

TEMP_DIR=$(mktemp -d -t install-$APP_NAME.XXXXXX)
trap 'rm -rf "$TEMP_DIR"' EXIT

echo "Downloading $ASSET_NAME..."
curl -sSfL -o "$TEMP_DIR/$ASSET_NAME" "$DOWNLOAD_URL"

echo "Extracting archive..."
tar -xzf "$TEMP_DIR/$ASSET_NAME" -C "$TEMP_DIR"

# ── 5. Install binary and platform integrations ────────────────────────────────
if [ "$OS" = "darwin" ]; then
    INSTALL_DIR="/Applications"
    if [ -d "$TEMP_DIR/Corvo.app" ]; then
        if [ -w "$INSTALL_DIR" ]; then
            rm -rf "$INSTALL_DIR/Corvo.app"
            mv "$TEMP_DIR/Corvo.app" "$INSTALL_DIR/"
        else
            echo "Administrator privileges required to write to $INSTALL_DIR:"
            sudo rm -rf "$INSTALL_DIR/Corvo.app"
            sudo mv "$TEMP_DIR/Corvo.app" "$INSTALL_DIR/"
        fi
    fi

    # Symlink to bin directory
    if [ -w "$BIN_DIR" ]; then
        rm -f "$BIN_DIR/corvo"
        ln -sf "$INSTALL_DIR/Corvo.app/Contents/MacOS/corvo" "$BIN_DIR/corvo"
    else
        sudo rm -f "$BIN_DIR/corvo"
        sudo ln -sf "$INSTALL_DIR/Corvo.app/Contents/MacOS/corvo" "$BIN_DIR/corvo"
    fi

    # Remove quarantine attribute, re-sign ad-hoc, and refresh LaunchServices
    xattr -cr "$INSTALL_DIR/Corvo.app" 2>/dev/null || true
    codesign --force --deep -s - "$INSTALL_DIR/Corvo.app" 2>/dev/null || true
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$INSTALL_DIR/Corvo.app" 2>/dev/null || true

    echo "Corvo.app installed to $INSTALL_DIR/Corvo.app"
    echo "Symlink created at $BIN_DIR/corvo"
else
    # Linux installation
    if [ -f "$TEMP_DIR/corvo" ]; then
        install -m 755 "$TEMP_DIR/corvo" "$BIN_DIR/corvo"
    elif [ -f "$TEMP_DIR/bin/corvo" ]; then
        install -m 755 "$TEMP_DIR/bin/corvo" "$BIN_DIR/corvo"
    fi

    # Install .desktop file
    DESKTOP_DIR="$HOME/.local/share/applications"
    mkdir -p "$DESKTOP_DIR"
    cat << 'EOF' > "$DESKTOP_DIR/corvo.desktop"
[Desktop Entry]
Type=Application
Name=Corvo
GenericName=Application Launcher
Comment=Lightweight native application launcher built with Rust & GPUI
Exec=corvo %U
Icon=corvo
Terminal=false
Categories=Utility;System;
Keywords=launcher;raycast;spotlight;alfred;command;search;app;
StartupWMClass=corvo
EOF
    chmod 644 "$DESKTOP_DIR/corvo.desktop"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    fi

    echo "Corvo binary installed to $BIN_DIR/corvo"
    echo "Desktop entry installed to $DESKTOP_DIR/corvo.desktop"
fi

# ── 6. Check PATH ─────────────────────────────────────────────────────────────
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo ""
        echo "WARNING: '$BIN_DIR' is not in your current PATH."
        echo "Add it to your shell configuration file (~/.zshrc, ~/.bashrc):"
        echo "    export PATH=\"$BIN_DIR:\$PATH\""
        ;;
esac

echo ""
echo "=== Corvo $LATEST_TAG installed successfully! ==="
echo "You can now launch Corvo from your application menu or by running 'corvo'."
