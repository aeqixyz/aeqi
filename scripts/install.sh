#!/usr/bin/env bash
# Install AEQI — downloads the latest pre-built binary for your platform.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/0xAEQI/aeqi/main/scripts/install.sh | sh
#
# Environment variables:
#   AEQI_VERSION     — Pin a specific version (e.g., v0.1.0). Default: latest.
#   AEQI_INSTALL_DIR — Install directory. Default: /usr/local/bin.

set -euo pipefail

REPO="0xAEQI/aeqi"
INSTALL_DIR="${AEQI_INSTALL_DIR:-/usr/local/bin}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64|amd64)  ARCH="amd64" ;;
  aarch64|arm64)  ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

case "$OS" in
  linux)  PLATFORM="linux" ;;
  darwin) PLATFORM="darwin" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

ARTIFACT="aeqi-${PLATFORM}-${ARCH}"

# Resolve version.
if [ -z "${AEQI_VERSION:-}" ]; then
  AEQI_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$AEQI_VERSION" ]; then
    echo "Failed to determine latest version." >&2
    exit 1
  fi
fi

URL="https://github.com/${REPO}/releases/download/${AEQI_VERSION}/${ARTIFACT}"

echo "Installing aeqi ${AEQI_VERSION} (${PLATFORM}/${ARCH})..."
curl -fsSL "$URL" -o /tmp/aeqi
chmod +x /tmp/aeqi

if [ -w "$INSTALL_DIR" ]; then
  mv /tmp/aeqi "$INSTALL_DIR/aeqi"
else
  sudo mv /tmp/aeqi "$INSTALL_DIR/aeqi"
fi

echo ""
echo "  aeqi installed to ${INSTALL_DIR}/aeqi"
echo ""
echo "  Get started:"
echo "    aeqi setup     # configure provider + API key"
echo "    aeqi start     # start daemon + dashboard on localhost:8400"
echo ""
