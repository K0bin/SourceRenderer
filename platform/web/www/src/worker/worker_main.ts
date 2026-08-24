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

onmessage = async (event: MessageEvent) => {
    const typedEvent = event.data as EngineWorkerMessage;
    const msgData = typedEvent.data;
    switch (typedEvent.messageType) {
        case EngineWorkerMessageType.TransferCanvas: {
            let canvas = msgData as OffscreenCanvas;
            receiveCanvas(canvas);
            await init(canvas);
        }
            break;
        case EngineWorkerMessageType.RequestCanvas: {
            let canvas = takeCanvas();
            (event.source as Worker).postMessage({
                    messageType: EngineWorkerMessageType.TransferCanvas,
                    data: canvas,
                } as EngineWorkerMessage,
                [canvas]
            );
        }
            break;
    }
};

console.log("EngineThread initialized");

// Values are in WASM pages (64KiB)
const memory = new WebAssembly.Memory({
    initial: 80,
    maximum: 16384,
    shared: true
});
await initWasm(undefined, memory);

postMessage({
    messageType: EngineWorkerMessageType.RequestCanvas,
} as EngineWorkerMessage);

let engine: Engine | null = null;

async function init(canvas: OffscreenCanvas) {
    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

onerror = (_e) => {
    engine?.free();
    engine = null;
};

function frame() {
    engine?.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}
