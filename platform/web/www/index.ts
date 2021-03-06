import * as wasm from "sourcerenderer_web";

wasm.greet();

if ('serviceWorker' in navigator) {
    // Use the window load event to keep the page load performant
    window.addEventListener('load', () => {
        // TODO: this doesnt trigger in Firefox for whatever reason
        navigator.serviceWorker.register('/service-worker.js').then(registration => {
            console.log('ServiceWorker registered: ', registration);
          }).catch(registrationError => {
            console.log('ServiceWorker registration failed: ', registrationError);
          });
    });
}
