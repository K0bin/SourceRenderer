import init, * as wasm from "../wasm_lib/sourcerenderer_web.js";


init().then(() => {
  wasm.greet();
});

if ('serviceWorker' in navigator) {
    // Use the window load event to keep the page load performant
    window.addEventListener('load', () => {
        // TODO: this doesnt trigger in Firefox for whatever reason
        navigator.serviceWorker.register('./src/service_worker.js').then(registration => {
            console.log('ServiceWorker registered: ', registration);
          }).catch(registrationError => {
            console.log('ServiceWorker registration failed: ', registrationError);
          });
    });
}
