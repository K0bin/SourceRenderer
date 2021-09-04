
const { startEngine } = wasm_bindgen;

start();

function start() {
  wasm_bindgen('libsourcerenderer.wasm')
      .then(() => {
          startEngine("canvas");
      })
      .catch((e) => {
          console.error("Failed initializing WASM: " + e);
      });
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
