import EngineWorker from './worker/worker_main.ts?worker'

function main() {
  const canvas = document.getElementById("canvas") as HTMLCanvasElement;
  canvas.width = window.innerWidth;
  canvas.height = window.innerHeight;
  const offscreenCanvas = canvas.transferControlToOffscreen();

  const worker = new EngineWorker({ name: "EngineThread" });
  worker.onmessage = (_e: MessageEvent) => {
    worker.postMessage({ canvas: offscreenCanvas }, [offscreenCanvas]);
    console.log("Sent canvas to worker");
  };
}

main();
