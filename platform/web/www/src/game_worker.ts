import * as wasm from "sourcerenderer_web";

let game: wasm.Game | null = null;

start();

function start() {
  game = wasm.gameWorkerMain(32);
}

console.log("game worker started.");
