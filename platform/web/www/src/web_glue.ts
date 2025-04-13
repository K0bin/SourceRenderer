import ThreadWorker from './worker/thread_worker?worker'

export async function fetchAsset(path: string): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin)
    console.trace("Fetching: " + url);
    const response = await fetch(url);
    if (response.status != 200) {
        throw response.status;
    }
    const buffer = await response.arrayBuffer();
    return new Uint8Array(buffer);
}

export interface ThreadWorkerInit {
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    callbackPtr: bigint,
    data: any,
}
export function startThreadWorker(
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    callbackPtr: bigint,
    data: any,
    name: string,
): Worker {
    const worker = new ThreadWorker({ name });
    const msg: ThreadWorkerInit = {
        module,
        memory,
        callbackPtr,
        data,
    };
    let transferables: Array<Transferable> = [];
    if (data instanceof OffscreenCanvas || data instanceof ArrayBuffer) {
        transferables.push(data);
    }
    worker.postMessage(msg, transferables);
    return worker;
}
