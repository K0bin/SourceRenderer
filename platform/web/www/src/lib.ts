// JS functions that are exposed to Rust

export function startGameWorker(): Worker {
  return new Worker(new URL("./../game_worker_bootstrap.js", import.meta.url));
}

export function startAssetWorker(): Worker {
  return new Worker(new URL("./../asset_worker_bootstrap.js", import.meta.url));
}
