function main() {
  const canvas = document.getElementById("canvas") as HTMLCanvasElement;
  canvas.width = window.innerWidth;
  canvas.height = window.innerHeight;
  const offscreenCanvas = canvas.transferControlToOffscreen();

  const worker = new Worker(new URL("./worker/worker_main.ts", import.meta.url), { name: "EngineThread", type: "module" });
  worker.onmessage = (_e: MessageEvent) => {
    worker.postMessage({ canvas: offscreenCanvas }, [offscreenCanvas]);
    console.log("Sent canvas to worker");
  };
}

main();