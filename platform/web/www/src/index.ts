
const { startEngine, WorkerPool, engineFrame, startRayonWorkers } = wasm_bindgen;

start();

let enginePtr: number = 0;

function start() {
  wasm_bindgen('libsourcerenderer.wasm')
      .then(() => {
        const pool = new WorkerPool(6);
        let rayonInit = startRayonWorkers(pool, 3);
        let intervalHandle = setInterval(() => {
          if (rayonInit.isDone()) {
            clearInterval(intervalHandle);
            enginePtr = startEngine("canvas", pool);
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
