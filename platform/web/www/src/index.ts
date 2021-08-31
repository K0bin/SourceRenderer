import * as wasm from "sourcerenderer_web";
import * as lib from "./lib.js";

let engine: wasm.WebEngine | null = null;

start();

function start() {
  const canvas = document.getElementById("canvas")! as HTMLCanvasElement;
  engine = wasm.startEngine(canvas);
}

if ('serviceWorker' in navigator) {
    // Use the window load event to keep the page load performant
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('./service_worker.bundle.js').then(registration => {
            console.log('ServiceWorker registered: ', registration);
          }).catch(registrationError => {
            console.log('ServiceWorker registration failed: ', registrationError);
          });
    });
}
