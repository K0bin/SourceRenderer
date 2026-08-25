import EngineWorker from './worker/worker_main.ts?worker'

let offscreenCanvas: OffscreenCanvas | null = null;

function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    offscreenCanvas = canvas.transferControlToOffscreen();

    const worker = new EngineWorker({name: "EngineThread"});
    worker.postMessage(offscreenCanvas, [offscreenCanvas]);
    offscreenCanvas = null;
}

main();
