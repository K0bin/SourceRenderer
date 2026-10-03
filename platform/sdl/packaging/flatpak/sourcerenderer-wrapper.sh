#!/bin/sh
set -e

APP_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/sourcerenderer"
mkdir -p "$APP_DATA_DIR"
cd "$APP_DATA_DIR"

if [ ! -e "shaders" ]; then
    ln -s /app/share/sourcerenderer/shaders shaders
fi

if [ ! -e "assets" ]; then
    ln -s /app/share/sourcerenderer/assets assets
fi

exec /app/bin/sourcerenderer_sdl "$@"
