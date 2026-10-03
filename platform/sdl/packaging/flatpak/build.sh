#!/bin/sh
set -e

BUILD_DIR="build"
REPO_DIR="repo"

wget -O flatpak-cargo-generator.py https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
python3 flatpak-cargo-generator.py ../../../../Cargo.lock -o cargo-sources.json

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

flatpak-builder --force-clean --user --install-deps-from=flathub --repo="$REPO_DIR" "$BUILD_DIR" io.github.k0bin.SourceRenderer.json "$@"
flatpak build-bundle repo toyrenderer.flatpak io.github.k0bin.SourceRenderer --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo

rm -rf "$BUILD_DIR"
rm -rf "$REPO_DIR"
