#!/usr/bin/env sh

set -e

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        PLATFORM="macos"
        ;;
    Linux)
        PLATFORM="linux"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)
        ARCH_TAG="x86_64"
        ;;
    arm64|aarch64)
        ARCH_TAG="arm64"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

NAME="semifold-${PLATFORM}-${ARCH_TAG}"
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

echo "[*] Downloading $NAME ..."
curl -L -o "$BIN_DIR/semifold" "https://github.com/noctisynth/semifold/releases/latest/download/${NAME}"

chmod +x "$BIN_DIR/semifold"

echo "[*] Installed semifold to $BIN_DIR"
echo "[*] Add $BIN_DIR to your PATH to use it."
