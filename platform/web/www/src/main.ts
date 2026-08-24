import EngineWorker from './worker/worker_main.ts?worker'
import ThreadWorker from './worker/thread_worker.ts?worker'
import {
    EngineWorkerMessageType,
    EngineWorkerMessage,
    takeCanvas
} from './engine_worker_communication';

let offscreenCanvas: OffscreenCanvas | null = null;

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    offscreenCanvas = canvas.transferControlToOffscreen();

    const worker = new EngineWorker({name: "EngineThread"});

    // Workaround for browser bugs
    worker.onmessage = (event) => {
        const typedEvent = event.data as EngineWorkerMessage;
        switch (typedEvent.messageType) {
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
        }
    };
}

main();
