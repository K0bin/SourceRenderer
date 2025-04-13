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
    initOutput = await initWasm({
        module_or_path: data.module,
        memory: data.memory,
    });
    await threadFunc(data.callbackPtr, data.data);
    initOutput.__wbindgen_thread_destroy();
}

onerror = (_e) => {
    initOutput?.__wbindgen_thread_destroy();
};
