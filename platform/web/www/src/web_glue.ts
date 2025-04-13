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
): Worker {
    const worker = new ThreadWorker({ name: "Thread" });
    const msg: ThreadWorkerInit = {
        module,
        memory,
        callbackPtr,
        data,
    };
    worker.postMessage(msg);
    return worker;
}
