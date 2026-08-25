import {default as initWasm, Engine, InitOutput, startEngine} from "../../../lib/pkg/sourcerenderer_web";
import {destroyThread, EngineWorkerMessage, EngineWorkerMessageType} from "../engine_worker_communication";


// Values are in WASM pages (64KiB)
const memory = new WebAssembly.Memory({
    initial: 80,
    maximum: 16384,
    shared: true
});

let thread: InitOutput | null = null;
let engine: Engine | null = null;

console.log("EngineThread initialized");

onmessage = async (event: MessageEvent) => {
    let msg = event.data as EngineWorkerMessage;
    switch (msg.messageType) {
        case EngineWorkerMessageType.InitMainThread: {
            thread
                = await initWasm({module_or_path: undefined, memory: memory});
            let canvas = msg.data as OffscreenCanvas;
            await init(canvas);
        }
            break;

        default:
            throw new Error("Unexpected message type on thread: " + msg.messageType);
    }
};

async function init(canvas: OffscreenCanvas) {
    if (engine !== null) {
        throw new Error("Engine already initialized.");
    }
    engine = await startEngine(navigator, canvas);
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
    engine?.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}
