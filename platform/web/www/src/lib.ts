// JS functions that are exposed to Rust

export function startGameWorker(): Worker {
  // Needs to be done in TS/JS so WebPack notices the WebWorker source file and bundles it
  return new Worker(new URL("./../game_worker_bootstrap.js", import.meta.url));
}

export function startAssetWorker(): Worker {
  // Needs to be done in TS/JS so WebPack notices the WebWorker source file and bundles it
  return new Worker(new URL("./../asset_worker_bootstrap.js", import.meta.url));
}
