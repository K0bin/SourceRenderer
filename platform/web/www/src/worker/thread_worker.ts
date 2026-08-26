import {default as initWasm, threadFunc, InitOutput, startEngine} from "../../../lib/pkg/sourcerenderer_web";

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

        case EngineWorkerMessageType.InitMainThread: {
            await initMain(msg.data as OffscreenCanvas);
        }
            break;

        default:
            throw new Error("Unexpected message type on thread: " + msg.messageType);
    }
};
console.log("Thread initialized");

let memory: WebAssembly.Memory | null = null;
let thread: InitOutput | null = null;
let engine: Engine | null = null;

async function initMain(canvas: OffscreenCanvas) {
    // Values are in WASM pages (64KiB)
    memory = new WebAssembly.Memory({
        initial: 80,
        maximum: 16384,
        shared: true
    });
    thread
        = await initWasm({module_or_path: undefined, memory: memory});

    if (engine !== null) {
        throw new Error("Engine already initialized.");
    }
    engine = await startEngine(canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

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
    engine?.free();
    if (thread !== null) {
        destroyThread(thread);
        thread = null;
    }
    engine = null;
};

function frame() {
    engine?.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}
