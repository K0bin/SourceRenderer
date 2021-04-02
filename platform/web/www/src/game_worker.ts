import * as wasm from "sourcerenderer_web";
import * as lib from "./lib.ts";

let game: wasm.Game | null = null;

start();

function start() {
  game = wasm.startGameWorker(32);
}

console.log("game worker started.");
