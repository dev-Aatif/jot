#!/bin/bash

set -e

# Configuration
REPO="dev-Aatif/jot"
BINARY_NAME="jotun"
INSTALL_DIR="$HOME/.local/bin"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Installing Jotun...${NC}"

# 1. Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux|darwin) ;;
    *)
        echo -e "${RED}❌ Jotun pre-built binaries currently support Linux and macOS only (got $OS).${NC}"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)
        echo -e "${RED}❌ Architecture $ARCH is not supported yet for pre-built binaries.${NC}"
        echo -e "Please install from source: https://github.com/$REPO#manual-from-source"
        exit 1
        ;;
esac

ASSET_NAME="jotun-${OS}-${ARCH}"

# 2. Get latest version from GitHub
echo -e "${BLUE}🔍 Fetching latest version...${NC}"
LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo -e "${RED}❌ Could not fetch latest release. Please check your internet connection.${NC}"
    exit 1
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$ASSET_NAME"
CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

# 3. Create install dir and temp staging directory
mkdir -p "$INSTALL_DIR"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# 4. Download binary and verify checksum
echo -e "${BLUE}📥 Downloading $LATEST_TAG ($ASSET_NAME)...${NC}"
curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME"

if curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/$ASSET_NAME.sha256"; then
    echo -e "${BLUE}🔐 Verifying SHA-256 checksum...${NC}"
    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$TMP_DIR" && sha256sum -c "$ASSET_NAME.sha256")
    elif command -v shasum >/dev/null 2>&1; then
        (cd "$TMP_DIR" && shasum -a 256 -c "$ASSET_NAME.sha256")
    else
        echo -e "${RED}⚠️  No sha256sum/shasum tool found; skipping checksum verification.${NC}"
    fi
else
    echo -e "${RED}⚠️  Checksum file not available for $LATEST_TAG; skipping verification.${NC}"
fi

chmod +x "$TMP_DIR/$ASSET_NAME"
mv "$TMP_DIR/$ASSET_NAME" "$INSTALL_DIR/$BINARY_NAME"

echo -e "${GREEN}✅ Jotun has been installed to $INSTALL_DIR/$BINARY_NAME${NC}"

# 5. Check if in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${RED}⚠️  $INSTALL_DIR is not in your PATH.${NC}"
    echo -e "Add this to your .bashrc or .zshrc:"
    echo -e "  export PATH=\"\$PATH:\$HOME/.local/bin\""
fi

echo -e "${BLUE}🎉 Done! Try running: jotun new \"Hello Jotun\"${NC}"
