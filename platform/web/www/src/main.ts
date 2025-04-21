import EngineWorker from './worker/worker_main.ts?worker'
import ThreadWorker from './worker/thread_worker.ts?worker'
import { EngineWorkerMessageType, ThreadWorkerInit, EngineWorkerMessage, FakeCanvasData } from './engine_worker_communication';

let offscreenCanvas: OffscreenCanvas|null = null;

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

            case EngineWorkerMessageType.RequestCanvas:
                const canvas = takeCanvas();
                worker.postMessage({
                        messageType: EngineWorkerMessageType.TransferCanvas,
                        data: canvas,
                    } as EngineWorkerMessage,
                    [canvas]
                );
                offscreenCanvas = null;
                break;

            case EngineWorkerMessageType.RequestFakeCanvas:
                worker.postMessage({
                    messageType: EngineWorkerMessageType.TransferFakeCanvas,
                    data: {
                        width: width,
                        height: height,
                    } as FakeCanvasData,
                } as EngineWorkerMessage);
                break;
        }
    };
}

function takeCanvas(): OffscreenCanvas {
    if (offscreenCanvas === null) {
        throw new Error("Canvas can only be transferred once.");
    }
    return offscreenCanvas;
}

function startThreadWorker(msg: ThreadWorkerInit) {
    console.info("Starting thread from main thread.");
    const worker = new ThreadWorker({ name: msg.name });
    let transferables: Array<Transferable> = [];
    if (msg.data === "FAKE_CANVAS") {
        msg.data = takeCanvas();
    }
    if (msg.data instanceof OffscreenCanvas || msg.data instanceof ArrayBuffer) {
        transferables.push(msg.data);
    }
    worker.postMessage(msg, transferables);
}

main();
