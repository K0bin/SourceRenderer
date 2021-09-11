// synchronously, using the browser, import out shim JS scripts
importScripts('libsourcerenderer_glue.js'); // TODO

// Wait for the main thread to send us the shared module/memory. Once we've got
// it, initialize it all with the `wasm_bindgen` global we imported via
// `importScripts`.
//
// After our first message all subsequent messages are an entry point to run,
// so we just do that.
self.onmessage = event => {
  wasm_bindgen(event.data[0], event.data[1]).catch(err => {
    // Propagate to main `onerror`:
    setTimeout(() => {
      throw err;
    });
    // Rethrow to keep promise rejected and prevent execution of further commands:
    throw err;
  }).then(() => {
    wasm_bindgen.worker_callback(event.data[2]);
  }).catch((e) => {
    console.error("Failed to initialize worker: " + e);
  });
};
