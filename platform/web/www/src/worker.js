// synchronously, using the browser, import out shim JS scripts
importScripts('libsourcerenderer_glue.js'); // TODO

// Wait for the main thread to send us the shared module/memory. Once we've got
// it, initialize it all with the `wasm_bindgen` global we imported via
// `importScripts`.
//
// After our first message all subsequent messages are an entry point to run,
// so we just do that.
self.onmessage = event => {
  let initialised = wasm_bindgen(event.data[0], event.data[1]).catch(err => {
    // Propagate to main `onerror`:
    setTimeout(() => {
      throw err;
    });
    // Rethrow to keep promise rejected and prevent execution of further commands:
    throw err;
  });

  const recycleWorker = event.data[2];

  self.onmessage = async event => {
    // This will queue further commands up until the module is fully initialised:
    await initialised;
    wasm_bindgen.child_entry_point(event.data, recycleWorker);
  };
};