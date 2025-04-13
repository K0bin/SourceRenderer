import { default as initWasm, threadFunc, InitOutput } from "../../../lib/pkg/sourcerenderer_web";

import { ThreadWorkerInit } from "../web_glue.ts";

onmessage = async (msg: MessageEvent) => {
    console.log("Receiving msg");
    await run(msg.data as ThreadWorkerInit);
};
postMessage({});
console.log("Thread initialized");

let initOutput: InitOutput|null = null;

async function run(data: ThreadWorkerInit) {
    console.log("Thread starting");

    initOutput = await initWasm({
        module_or_path: data.module,
        memory: data.memory,
    });
    await threadFunc(data.callbackPtr, data.data);
    console.log("Thread finished");
}

onerror = (_e) => {
    destroyThread();
};

export function destroyThread() {
    // This has to be done in a separate function because
    // the Promise -> Rust conversion happens after run is done.
    // It also has to happen inside of JS.
    console.log("Destroying thread");
    initOutput?.__wbindgen_thread_destroy();
    initOutput = null;
    console.log("Thread destroyed");
}