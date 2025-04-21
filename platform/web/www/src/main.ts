import EngineWorker from './worker/worker_main.ts?worker'
import ThreadWorker from './worker/thread_worker.ts?worker'
import { EngineWorkerMessageType, ThreadWorkerInit, EngineWorkerMessage, FakeCanvasData } from './engine_worker_communication';

let offscreenCanvas: OffscreenCanvas|null = null;

// Must be true if the render thread feature isn't enabled.
// Should be false if it is enabled because of browser bugs.
const TRANSFER_CANVAS: boolean = true;

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    const width = canvas.width;
    const height = canvas.height;
    offscreenCanvas = canvas.transferControlToOffscreen();

    const worker = new EngineWorker({ name: "EngineThread" });

    // Workaround for browser bugs
    worker.onmessage = (event) => {
        const typedEvent = event.data as EngineWorkerMessage;
        switch (typedEvent.messageType) {
            case EngineWorkerMessageType.StartThreadFromMain:
                startThreadWorker(typedEvent.data as ThreadWorkerInit);
                break;
        }
    };

    if (TRANSFER_CANVAS) {
        worker.postMessage({
                messageType: EngineWorkerMessageType.TransferCanvas,
                data: offscreenCanvas,
            } as EngineWorkerMessage,
            [offscreenCanvas]
        );
    } else {
        worker.postMessage({
            messageType: EngineWorkerMessageType.TransferFakeCanvas,
            data: {
                width: width,
                height: height,
            } as FakeCanvasData,
        } as EngineWorkerMessage);
    }
}

function startThreadWorker(msg: ThreadWorkerInit) {
    console.info("Starting thread from main thread.");
    const worker = new ThreadWorker({ name: msg.name });
    let transferables: Array<Transferable> = [];
    if (msg.data === "FAKE_CANVAS") {
        msg.data = offscreenCanvas;
        offscreenCanvas = null;
    }
    if (msg.data instanceof OffscreenCanvas || msg.data instanceof ArrayBuffer) {
        transferables.push(msg.data);
    }
    worker.postMessage(msg, transferables);
}

main();
