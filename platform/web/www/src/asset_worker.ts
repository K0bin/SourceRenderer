import * as wasm from "sourcerenderer_web";

let engine: wasm.WebEngine | null = null;

start();

function start() {
}

self.onmessage = (msg: MessageEvent) => {

};

console.log("Asset worker started.");
