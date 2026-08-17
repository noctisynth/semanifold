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

normalize_version() {
    normalized_version="${1#v}"
    if ! printf '%s\n' "$normalized_version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'; then
        echo "Invalid version: $1" >&2
        return 1
    fi
    printf '%s\n' "$normalized_version"
}

resolve_latest_version() {
    page=1
    while :; do
        releases_url="https://github.com/noctisynth/semifold/releases?page=$page"
        releases_html="$(curl -fsSL "$releases_url")" || return 1
        latest_version="$(printf '%s\n' "$releases_html" \
            | grep -Eo '/noctisynth/semifold/releases/tag/semifold-v[0-9]+\.[0-9]+\.[0-9]+"' \
            | sed -n 's#^/noctisynth/semifold/releases/tag/semifold-v\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$#\1#p' \
            | sed -n '1p')"
        if [ -n "$latest_version" ]; then
            printf '%s\n' "$latest_version"
            return 0
        fi
        if ! printf '%s\n' "$releases_html" | grep -q 'rel="next"'; then
            echo "No stable Semifold binary release was found" >&2
            return 1
        fi
        page=$((page + 1))
    done
}

if [ -n "$VERSION" ]; then
    VERSION="$(normalize_version "$VERSION")" || exit 1
else
    echo "[*] Resolving the latest stable Semifold release ..."
    if ! VERSION="$(resolve_latest_version)"; then
        echo "Failed to resolve the latest stable Semifold release" >&2
        exit 1
    fi
fi

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

RELEASE_PATH="download/semifold-v${VERSION}"

echo "[*] Downloading $NAME version $VERSION ..."
trap 'rm -f "$TEMP_FILE"' EXIT HUP INT TERM
curl -fL -o "$TEMP_FILE" "https://github.com/noctisynth/semifold/releases/${RELEASE_PATH}/${NAME}"

chmod +x "$TEMP_FILE"
mv "$TEMP_FILE" "$DESTINATION"
trap - EXIT HUP INT TERM

echo "[*] Installed semifold to $BIN_DIR"
echo "[*] Add $BIN_DIR to your PATH to use it."
