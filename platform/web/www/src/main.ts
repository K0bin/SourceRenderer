import ThreadWorker from './worker/thread_worker.ts?worker'
import {EngineWorkerMessage, EngineWorkerMessageType, ThreadWorkerInit} from "./engine_worker_communication.ts";

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;
    let offscreenCanvas: OffscreenCanvas | null = canvas.transferControlToOffscreen();
    const msg: EngineWorkerMessage = {
        messageType: EngineWorkerMessageType.InitMainThread,
        data: offscreenCanvas
    };

    let worker: Worker | null = new ThreadWorker({name: "EngineThread"});
    worker.postMessage(msg, [offscreenCanvas]);
    offscreenCanvas = null;

    worker!.onmessage = async (event: MessageEvent) => {
        const msg = event.data as EngineWorkerMessage;
        switch (msg.messageType) {
            case EngineWorkerMessageType.StartRenderThread: {
                const threadMsg = msg.data as ThreadWorkerInit;
                msg.messageType = EngineWorkerMessageType.InitThread;
                const renderWorker = new ThreadWorker({name: "RenderThread"});
                renderWorker.postMessage(msg, [threadMsg.data]);
                renderWorker.onerror = (e) => {
                    console.error("Error on render worker: ", e);
                };
                return;
            }

            default:
                throw new Error("Unexpected message type: " + msg.messageType);
        }
    };

    worker.onerror = (e) => {
        console.error("Error on render worker: ", e);
        worker = null;
    };

    const canvasSizeObserver = new ResizeObserver((entries, _observer) => {
        if (entries.length !== 1)
            console.warn("Unexpected resize observer entries size: " + entries.length);

        const width = entries[0].contentRect.width;
        const height = entries[0].contentRect.height;

        const msg: EngineWorkerMessage = {
            messageType: EngineWorkerMessageType.CanvasResized,
            data: {
                width,
                height,
            }
        };
        worker?.postMessage(msg);
    });
    canvasSizeObserver.observe(canvas, {
        box: "device-pixel-content-box"
    });
}

main();
