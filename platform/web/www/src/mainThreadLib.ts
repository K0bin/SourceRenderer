let webglServer: WebGLServer | null = null;
let workerPool = new Array<PooledWorker>();

class PooledWorker {
  public readonly worker: Worker;
  public isBusy: boolean;
  public readonly workerId: number;

  public constructor(workerId: number, onMessageCallback: ((this: Worker, ev: MessageEvent) => any)) {
    this.workerId = workerId;
    this.worker = new Worker("worker.js");
    this.worker.onmessage = onMessageCallback;
    this.isBusy = false;
  }
}

function populateWorkerPool(poolSize: number, module: WebAssembly.Module, memory: WebAssembly.Memory) {
  for (let i = 0; i < poolSize; i++) {
    let worker = new PooledWorker(i, onWorkerMessage);
    worker.worker.postMessage(new StartWorkerCommand(worker.workerId, module, memory));
    workerPool.push(worker);
  }
}

function startThread(functionPointer: number) {
  let worker: PooledWorker|null = null;
  for (let w of workerPool) {
    if (!w.isBusy) {
      worker = w;
      break;
    }
  }

  if (worker === null) {
    console.error("Failed to retrieve worker.");
    return;
  }

  worker.isBusy = true;
  worker.worker.postMessage(new WorkerWorkCommand(functionPointer));
}

function initWebGLServer(canvas: HTMLCanvasElement) {
  webglServer = new WebGLServer(canvas);
}

function onWorkerMessage(event: MessageEvent) {
  if (!event.data) {
    console.error("Broken worker message");
    console.error(event.data);
    return;
  }

  if (event.data.commandType === ReturnWorkerCommand.COMMAND_TYPE) {
    workerPool[event.data.workerId].isBusy = false;
  }

  webglServer?.tryExecute(event.data);
}
