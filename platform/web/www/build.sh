#!/bin/bash

# Build Rust code
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' wasm-pack build --target no-modules --release ../lib -Z build-std=std,panic_abort
cp ../lib/pkg/sourcerenderer_web_bg.wasm dist/libsourcerenderer.wasm
cp ../lib/pkg/sourcerenderer_web.js dist/libsourcerenderer_glue.js

# Build web code
tsc
cp index.html dist/
cp manifest.json dist/
cp src/worker.js dist/
