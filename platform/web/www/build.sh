#!/bin/bash

# Build Rust code
cd ../lib
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' wasm-pack build --target no-modules --dev -- -Z build-std=std,panic_abort
cd ../www
cp ../lib/pkg/sourcerenderer_web_bg.wasm dist/libsourcerenderer.wasm
cp ../lib/pkg/sourcerenderer_web.js dist/libsourcerenderer_glue.js

# Build web code
tsc
tsc -p tsconfig.worker.json
cp index.html dist/
cp manifest.json dist/
