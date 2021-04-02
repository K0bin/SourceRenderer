import * as wasm from "sourcerenderer_web";
import * as lib from "./lib.ts";

let engine: wasm.WebEngine | null = null;

start();

function start() {
}

self.onmessage = (msg: MessageEvent) => {

};

console.log("game worker started.");
