import { default as initWasm, Engine, startEngine, startEngineWithFakeCanvas } from "../../../lib/pkg/sourcerenderer_web";
import { EngineWorkerMessage, EngineWorkerMessageType, FakeCanvasData, ThreadWorkerInit } from "../engine_worker_communication.ts";

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

let engine: Engine|null = null;

async function init(canvas: OffscreenCanvas) {
    await initWasm();
    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

async function initWithFakeCanvas(width: number, height: number) {
    await initWasm();
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
