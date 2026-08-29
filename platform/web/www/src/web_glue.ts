import ThreadWorker from './worker/thread_worker?worker'
import {EngineWorkerMessage, EngineWorkerMessageType, ThreadWorkerInit} from './engine_worker_communication';

export async function fetchAsset(path: string): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin);
    console.trace("Fetching: " + url);
    const response = await fetch(url);
    if (response.status != 200) {
        throw response.status;
    }
    return await response.bytes();
}

export async function fetchAssetRange(path: string, offset: number, length: number): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin);
    console.trace("Fetching: " + url);
    const response = await fetch(url, {
        headers: [
            ["Range", "bytes=" + offset + "-" + (offset + length)],
        ]
    });
    if (response.status != 200 && response.status != 206) {
        throw response.status;
    }
    return await response.bytes();
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
    callbackPtr: number,
    data: any,
    name: string,
) {
    const init: ThreadWorkerInit = {
        module,
        memory,
        callbackPtr,
        data,
        name,
    };
    const msg: EngineWorkerMessage = {
        messageType: EngineWorkerMessageType.InitThread,
        data: init
    };

    let transferables: Array<Transferable> = [];
    if (data instanceof OffscreenCanvas || data instanceof ArrayBuffer) {
        transferables.push(data);

        if (data instanceof OffscreenCanvas && isBlink()) {
            // https://issues.chromium.org/issues/41483010
            console.warn("Working around annoying Chrome bug.");

            msg.messageType = EngineWorkerMessageType.StartRenderThread;
            const scope = self as DedicatedWorkerGlobalScope;
            scope.postMessage(msg, transferables);
            return;
        }
    }

    const worker = new ThreadWorker({name});
    worker.postMessage(msg, transferables);
}

function isBlink() {
    const ua = self.navigator.userAgent;
    const isChrome = /Chrome/.test(ua);
    const isEdge = /Edg/.test(ua);
    const isOpera = /OPR/.test(ua);
    const isVivaldi = /Vivaldi/.test(ua);

    return isChrome || isEdge || isOpera || isVivaldi;
}
