#!/usr/bin/env sh

set -e

usage() {
    echo "Usage: install.sh [version] [--install-dir <path>]" >&2
}

VERSION=""
BIN_DIR="$HOME/.local/bin"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --install-dir)
            if [ "$#" -lt 2 ] || [ -z "$2" ]; then
                usage
                exit 1
            fi
            BIN_DIR="$2"
            shift 2
            ;;
        --install-dir=*)
            BIN_DIR="${1#*=}"
            if [ -z "$BIN_DIR" ]; then
                usage
                exit 1
            fi
            shift
            ;;
        --*)
            usage
            exit 1
            ;;
        *)
            if [ -n "$VERSION" ]; then
                usage
                exit 1
            fi
            VERSION="$1"
            shift
            ;;
    esac
done

case "$VERSION" in
    *[!0-9A-Za-z.+-]*)
        echo "Invalid version: $VERSION" >&2
        exit 1
        ;;
esac

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
DESTINATION="$BIN_DIR/semifold"
TEMP_FILE="$BIN_DIR/.semifold.tmp.$$"
mkdir -p "$BIN_DIR"

if [ -n "$VERSION" ]; then
    RELEASE_PATH="download/semifold-${VERSION}"
else
    RELEASE_PATH="latest/download"
fi

echo "[*] Downloading $NAME${VERSION:+ version $VERSION} ..."
trap 'rm -f "$TEMP_FILE"' EXIT HUP INT TERM
curl -fL -o "$TEMP_FILE" "https://github.com/noctisynth/semifold/releases/${RELEASE_PATH}/${NAME}"

chmod +x "$TEMP_FILE"
mv "$TEMP_FILE" "$DESTINATION"
trap - EXIT HUP INT TERM

echo "[*] Installed semifold to $BIN_DIR"
echo "[*] Add $BIN_DIR to your PATH to use it."
