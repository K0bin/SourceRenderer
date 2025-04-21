import { default as initWasm, Engine, startEngine, startEngineWithFakeCanvas, hasRenderThread } from "../../../lib/pkg/sourcerenderer_web";
import { EngineWorkerMessage, EngineWorkerMessageType, FakeCanvasData } from "../engine_worker_communication.ts";

onmessage = async (event: MessageEvent) => {
    const typedEvent = event.data as EngineWorkerMessage;
    const msgData = typedEvent.data;
    switch (typedEvent.messageType) {
        case EngineWorkerMessageType.TransferCanvas:
            let canvas = msgData as OffscreenCanvas;
            await init(canvas);
            break;

        case EngineWorkerMessageType.TransferFakeCanvas:
            let fakeCanvasData = msgData as FakeCanvasData;
            await initWithFakeCanvas(fakeCanvasData.width, fakeCanvasData.height);
            break;
    }
};

console.log("EngineThread initialized");

await initWasm();

if (hasRenderThread()) {
    postMessage({
        messageType: EngineWorkerMessageType.RequestFakeCanvas,
    } as EngineWorkerMessage);
} else {
    postMessage({
        messageType: EngineWorkerMessageType.RequestCanvas,
    } as EngineWorkerMessage);
}

let engine: Engine|null = null;

async function init(canvas: OffscreenCanvas) {
    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

async function initWithFakeCanvas(width: number, height: number) {
    engine = await startEngineWithFakeCanvas(navigator, width, height);
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
