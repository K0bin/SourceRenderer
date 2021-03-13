/// <reference path="../dist/wasm_lib/sourcerenderer_web.d.ts" />

let engine: wasm_bindgen.WebEngine | null = null;

loadWebAssemblyAndStart();

async function loadWebAssemblyAndStart() {
  await wasm_bindgen("../wasm_lib/sourcerenderer_web_bg.wasm");
  const canvas = document.getElementById("canvas")! as HTMLCanvasElement;
  engine = wasm_bindgen.startEngine(canvas);
  animationFrame(0);
}

function animationFrame(time: DOMHighResTimeStamp) {
  wasm_bindgen.render(engine!);
  window.requestAnimationFrame(animationFrame);
}

if ('serviceWorker' in navigator) {
    // Use the window load event to keep the page load performant
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('./service_worker.js').then(registration => {
            console.log('ServiceWorker registered: ', registration);
          }).catch(registrationError => {
            console.log('ServiceWorker registration failed: ', registrationError);
          });
    });
}
