#!/usr/bin/env bash
set -e

INSTALL_DIR="$HOME/.local"
BUILD_DIR="$HOME/.siege-build"
SIEGE_VERSION="4.1.6"

mkdir -p "$BUILD_DIR" "$INSTALL_DIR/bin"

cd "$BUILD_DIR"

echo "Downloading siege..."
curl -L "http://download.joedog.org/siege/siege-${SIEGE_VERSION}.tar.gz" -o siege.tar.gz
tar xzf siege.tar.gz
cd "siege-${SIEGE_VERSION}"

echo "Configuring..."
./configure --prefix="$INSTALL_DIR"

echo "Building..."
make -j"$(nproc)"

echo "Installing to $INSTALL_DIR..."
make install

echo "Cleaning up..."
rm -rf "$BUILD_DIR"

# Add to PATH if not already there
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc" 2>/dev/null || true
  export PATH="$HOME/.local/bin:$PATH"
fi

echo "Done. Verify with: siege --version"