import {
    default as initWasm,
    Engine,
    startEngine,
    startEngineWithFakeCanvas,
    hasRenderThread
} from "../../../lib/pkg/sourcerenderer_web";
import {
    EngineWorkerMessage,
    EngineWorkerMessageType,
    FakeCanvasData,
    receiveCanvas, takeCanvas
} from "../engine_worker_communication.ts";


// Values are in WASM pages (64KiB)
const memory = new WebAssembly.Memory({
    initial: 80,
    maximum: 16384,
    shared: true
});
await initWasm(undefined, memory);

console.log("EngineThread initialized");

onmessage = async (event: MessageEvent) => {
    let canvas = event.data as OffscreenCanvas;
    await init(canvas);
};

let engine: Engine | null = null;

async function init(canvas: OffscreenCanvas) {
    if (engine !== null) {
        throw new Exception("Engine already initialized.");
    }
    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

onerror = (e) => {
    console.error(e);
    engine?.free();
    engine = null;
};

function frame() {
    engine?.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}
