import {default as initWasm, threadFunc, InitOutput} from "../../../lib/pkg/sourcerenderer_web";

import {
    destroyThread,
    EngineWorkerMessage,
    EngineWorkerMessageType,
    ThreadWorkerInit
} from "../engine_worker_communication.ts";

onmessage = async (event: MessageEvent) => {
    let msg = event.data as EngineWorkerMessage;
    switch (msg.messageType) {
        case EngineWorkerMessageType.InitThread: {
            await run(msg.data as ThreadWorkerInit);
        }
            break;

        default:
            throw new Error("Unexpected message type on main thread: " + msg.messageType);
    }
};
console.log("Thread initialized");

let thread: InitOutput | null = null;

async function run(data: ThreadWorkerInit) {
    console.log("Thread starting with payload:");
    console.log(data);

    thread = await initWasm({module_or_path: data.module, memory: data.memory});
    await threadFunc(data.callbackPtr, data.data);
    console.log("Thread finished");
    if (thread !== null) {
        destroyThread(thread);
        thread = null;
    }
}

onerror = (e) => {
    console.error(e);
    if (thread !== null) {
        destroyThread(thread);
        thread = null;
    }
};
