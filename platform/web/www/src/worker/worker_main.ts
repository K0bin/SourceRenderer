import { default as initWasm, Engine, startEngine } from "../../../lib/pkg/sourcerenderer_web";

onmessage = async (msg: MessageEvent) => {
    console.log("Receiving msg");
    let canvas = msg.data.canvas as OffscreenCanvas;
    await init(canvas);
};
postMessage({});
console.log("EngineThread initialized");

let engine: Engine|null = null;

async function init(canvas: OffscreenCanvas) {
    await initWasm();
    engine = await startEngine(navigator, canvas);
    requestAnimationFrame((_time) => {
        renderFrame();
    });
}

onerror = (_e) => {
    engine?.free();
    engine = null;
};

function renderFrame() {
    engine?.frame();

    requestAnimationFrame((_time) => {
        renderFrame();
    });
}
