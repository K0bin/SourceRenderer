import ThreadWorker from './worker/thread_worker?worker'
import { EngineWorkerMessage, EngineWorkerMessageType, ThreadWorkerInit } from './engine_worker_communication';

export async function fetchAsset(path: string): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin);
    console.trace("Fetching: " + url);
    const response = await fetch(url);
    if (response.status != 200) {
        throw response.status;
    }
    const buffer = await response.bytes();
    return buffer;
}

export async function fetchAssetRange(path: string, offset: number, length: number): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin);
    console.trace("Fetching: " + url);
    const response = await fetch(url, {
        headers: [
            ["Range", "bytes=" + offset + "-" + (offset + length)],
        ]
    });
    if (response.status != 200) {
        throw response.status;
    }
    const buffer = await response.bytes();
    return buffer;
}

export async function fetchAssetHead(path: string): Promise<number> {
    const url = new URL("./enginedata/" + path, location.origin);
    console.trace("Fetching HEADER: " + url);
    const response = await fetch(url, {
        method: "HEAD",
    });
    if (response.status !== 204 && response.status !== 200) {
        throw response.status;
    }
    const contentLength = response.headers.get("Content-Length");
    if (contentLength === null) {
        throw new Error("No content-length header");
    }
    return Number.parseInt(contentLength);
}

export function startThreadWorker(
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    callbackPtr: bigint,
    data: any,
    name: string,
) {
    const msg: ThreadWorkerInit = {
        module,
        memory,
        callbackPtr,
        data,
        name,
    };
    if (data === "FAKE_CANVAS") {
        console.warn("Starting thread from main thread as a browser bug workaround.");
        // Start the thread from the main thread.
        // This will break if this is a nested thread but it's just an ugly hack
        // workaround for browser bugs.
        postMessage({
            messageType: EngineWorkerMessageType.StartThreadFromMain,
            data: msg,
        } as EngineWorkerMessage);
        return;
    }
    const worker = new ThreadWorker({ name });
    let transferables: Array<Transferable> = [];
    if (data instanceof OffscreenCanvas || data instanceof ArrayBuffer) {
        transferables.push(data);
    }
    worker.postMessage(msg, transferables);
}
