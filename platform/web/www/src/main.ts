import EngineWorker from './worker/worker_main.ts?worker'
import ThreadWorker from './worker/thread_worker.ts?worker'
import {
    EngineWorkerMessage,
    EngineWorkerMessageType,
    ThreadWorkerInit
} from "./engine_worker_communication.ts";
import {Engine, startEngine, default as initWasm} from "sourcerenderer_web";

let engine: Engine | null = null;

async function main() {
    const canvas = document.getElementById("canvas") as HTMLCanvasElement;
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    // Values are in WASM pages (64KiB)
    const memory = new WebAssembly.Memory({
        initial: 80,
        maximum: 16384,
        shared: true
    });
    await initWasm({module_or_path: undefined, memory: memory});

    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        frame();
    });
}

function frame() {
    engine?.frame();

    requestAnimationFrame((_time) => {
        frame();
    });
}

await main();


onerror = (e) => {
    console.error(e);
    engine?.free();
    engine = null;
};