import EngineWorker from './worker/worker_main.ts?worker'
import ThreadWorker from './worker/thread_worker.ts?worker'
import {EngineWorkerMessage, EngineWorkerMessageType, ThreadWorkerInit} from "./engine_worker_communication.ts";

let offscreenCanvas: OffscreenCanvas | null = null;
let worker: Worker | null = null;

onmessage = async (event: MessageEvent) => {
    if (event.source != worker || worker === null)
        return;

    const msg = event.data as EngineWorkerMessage;
    switch (msg.messageType) {
        case EngineWorkerMessageType.StartRenderThread: {
            const threadMsg = msg.data as ThreadWorkerInit;
            const worker = new ThreadWorker({name: "RenderThread"});
            worker.postMessage(threadMsg, threadMsg.data);
            return;
        }

        default:
            throw new Error("Unexpected message type: " + msg.messageType);
    }
};

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    offscreenCanvas = canvas.transferControlToOffscreen();
    const msg: EngineWorkerMessage = {
        messageType: EngineWorkerMessageType.InitMainThread,
        data: offscreenCanvas
    };

    worker = new EngineWorker({name: "EngineThread"});
    worker.postMessage(msg, [offscreenCanvas]);
    offscreenCanvas = null;
}

main();
