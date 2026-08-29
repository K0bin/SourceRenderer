import {default as initWasm, Engine, InitOutput, startEngine} from "../../../lib/pkg/sourcerenderer_web";

import {
    CanvasResized,
    destroyThread,
    EngineWorkerMessage,
    EngineWorkerMessageType,
    MouseMoved
} from "../engine_worker_communication.ts";

onmessage = async (event: MessageEvent) => {
    let msg = event.data as EngineWorkerMessage;
    switch (msg.messageType) {
        case EngineWorkerMessageType.InitMainThread: {
            await initMain(msg.data as OffscreenCanvas);
        }
            break;

        case EngineWorkerMessageType.CanvasResized: {
            const data = msg.data as CanvasResized;
            engine
                ?.windowResized(data.width, data.height);
        }
            break;

        case EngineWorkerMessageType.MouseMoved: {
            const data = msg.data as MouseMoved;
            engine
                ?.mouseMoved(data.deltaX, data.deltaY);
        }
            break;

        case EngineWorkerMessageType.KeyUp:
        case EngineWorkerMessageType.KeyDown: {
            const data = msg.data as string;
            engine
                ?.keyboardEvent(msg.messageType == EngineWorkerMessageType.KeyDown, data);
        }
            break;

        default:
            throw new Error("Unexpected message type on thread: " + msg.messageType);
    }
};
console.log("Main worker initialized");

let memory: WebAssembly.Memory | null = null;
let thread: InitOutput | null = null;
let lastMouseLock = false;
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
    if (engine === null)
        return;

    if (engine.isMouseLocked() != lastMouseLock) {
        lastMouseLock = engine.isMouseLocked();
        const msg: EngineWorkerMessage = {
            messageType: EngineWorkerMessageType.UpdateMouseLock,
            data: lastMouseLock
        }
        postMessage(msg);
    }

    engine.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}
