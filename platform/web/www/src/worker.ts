importScripts('libsourcerenderer_glue.js');
importScripts('command.js');

let workerId: number|null = null;

// Wait for the main thread to send us the shared module/memory. Once we've got
// it, initialize it all with the `wasm_bindgen` global we imported via
// `importScripts`.
//
// After our first message all subsequent messages are an entry point to run,
// so we just do that.
self.onmessage = async event => {
  if (event.data.commandType === StartWorkerCommand.COMMAND_TYPE) {
    let command = event.data as StartWorkerCommand;
    workerId = command.workerId;
    let initialised = wasm_bindgen(command.module, command.memory).catch(err => {
      // Propagate to main `onerror`:
      setTimeout(() => {
        throw err;
      });
      // Rethrow to keep promise rejected and prevent execution of further commands:
      throw err;
    });
    self.onmessage = async event => {
      await initialised;
      onMainThreadMessage(event);
    };
  }
};

function onMainThreadMessage(event: MessageEvent) {
  if (event.data.commandType === WorkerWorkCommand.COMMAND_TYPE) {
    let command = event.data as WorkerWorkCommand;
    wasm_bindgen.child_entry_point(command.functionPointer);
  }
  self.postMessage(new ReturnWorkerCommand(workerId!));
}