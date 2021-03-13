/// <reference path="../dist/wasm_lib/sourcerenderer_web.d.ts" />

self.importScripts("../wasm_lib/sourcerenderer_web.js");

loadWebAssemblyAndStart();

async function loadWebAssemblyAndStart() {
  await wasm_bindgen("../wasm_lib/sourcerenderer_web_bg.wasm");
  // TODO: do stuff
}

console.log("AssetWorker started.");
