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

echo -e "${BLUE}🚀 Installing Jot...${NC}"

# 1. Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" != "linux" ]; then
    echo -e "${RED}❌ Jot v0.1.0 currently supports Linux only.${NC}"
    exit 1
fi

if [ "$ARCH" != "x86_64" ]; then
    echo -e "${RED}❌ Architecture $ARCH is not supported yet for pre-built binaries.${NC}"
    echo -e "Please install from source: https://github.com/$REPO#manual-from-source"
    exit 1
fi

# 2. Get latest version from GitHub
echo -e "${BLUE}🔍 Fetching latest version...${NC}"
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo -e "${RED}❌ Could not fetch latest release. Please check your internet connection.${NC}"
    exit 1
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/jotun-linux-x86_64"

# 3. Create install dir if it doesn't exist
mkdir -p "$INSTALL_DIR"

# 4. Download binary
echo -e "${BLUE}📥 Downloading $LATEST_TAG...${NC}"
curl -sSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo -e "${GREEN}✅ Jotun has been installed to $INSTALL_DIR/$BINARY_NAME${NC}"

# 5. Check if in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${RED}⚠️  $INSTALL_DIR is not in your PATH.${NC}"
    echo -e "Add this to your .bashrc or .zshrc:"
    echo -e "  export PATH=\"\$PATH:\$HOME/.local/bin\""
fi

echo -e "${BLUE}🎉 Done! Try running: jotun new \"Hello Jotun\"${NC}"
