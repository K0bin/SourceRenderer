import ThreadWorker from './worker/thread_worker.ts?worker'
import MainWorker from './worker/main_worker.ts?worker'
import {EngineWorkerMessage, EngineWorkerMessageType, ThreadWorkerInit} from "./engine_worker_communication.ts";

let mouseLocked = false;
let fullscreen = false;

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = canvas.clientWidth;
    canvas.height = canvas.clientHeight;
    let offscreenCanvas: OffscreenCanvas | null = canvas.transferControlToOffscreen();
    const msg: EngineWorkerMessage = {
        messageType: EngineWorkerMessageType.InitMainThread,
        data: offscreenCanvas
    };

    let worker: Worker | null = new MainWorker({name: "EngineThread"});
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

            case EngineWorkerMessageType.UpdateMouseLock: {
                mouseLocked = msg.data as boolean;
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

    canvas.onmousemove = (e) => {
        const msg: EngineWorkerMessage = {
            messageType: EngineWorkerMessageType.MouseMoved,
            data: {
                x: e.x,
                y: e.y,
                deltaX: e.movementX,
                deltaY: e.movementY
            }
        };
        worker?.postMessage(msg);
    };

    canvas.onkeydown = (e) => {
        const msg: EngineWorkerMessage = {
            messageType: EngineWorkerMessageType.KeyDown,
            data: e.code
        };
        worker?.postMessage(msg);
    };

    canvas.onkeyup = (e) => {
        const msg: EngineWorkerMessage = {
            messageType: EngineWorkerMessageType.KeyUp,
            data: e.code
        };
        worker?.postMessage(msg);
    };

    canvas.onmousedown = async (_e) => {
        if (mouseLocked) {
            try {
                await canvas.requestPointerLock();
            } catch (_e) {
            }
        }
    }

    canvas.focus();
}

main();
