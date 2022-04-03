
const { startEngine, WorkerPool, engineFrame, startRayonWorkers } = wasm_bindgen;

start();

let enginePtr: number = 0;

function start() {
  const canvas = document.querySelector("#canvas") as HTMLCanvasElement;
  resizeCanvasToDisplaySize(canvas);

  wasm_bindgen('libsourcerenderer.wasm')
      .then(() => {
        const pool = new WorkerPool(6);
        let rayonInit = startRayonWorkers(pool, 1);
        let intervalHandle = setInterval(() => {
          if (rayonInit.isDone()) {
            clearInterval(intervalHandle);
            enginePtr = startEngine(canvas, pool);
            requestAnimationFrame(frame);
          }
        }, 20);
      })
      .catch((e) => {
          console.error("Failed initializing WASM: " + e);
      });
}

function frame() {
  if (enginePtr !== 0) {
    const continueFrames = engineFrame(enginePtr);
    if (!continueFrames) {
      enginePtr = 0;
    }
    requestAnimationFrame(frame);
  }
}

function resizeCanvasToDisplaySize(canvas: HTMLCanvasElement) {
  // look up the size the canvas is being displayed
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;

  // If it's resolution does not match change it
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
    return true;
  }

  return false;
}

/*if ('serviceWorker' in navigator) {
    // Use the window load event to keep the page load performant
    window.addEventListener('load', () => {
        navigator.serviceWorker.register('./service_worker.bundle.js').then(registration => {
            console.log('ServiceWorker registered: ', registration);
          }).catch(registrationError => {
            console.log('ServiceWorker registration failed: ', registrationError);
          });
    });
}*/
