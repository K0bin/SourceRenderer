import EngineWorker from './worker/worker_main.ts?worker'
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
    worker.postMessage({}, [offscreenCanvas]);
    offscreenCanvas = null;
}

main();
