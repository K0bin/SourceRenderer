export enum EngineWorkerMessageType {
    TransferCanvas,
    RequestCanvas,
}

let offscreenCanvas: OffscreenCanvas | null = null;

export function takeCanvas(): OffscreenCanvas {
    if (offscreenCanvas === null) {
        throw new Error("Canvas can only be transferred once.");
    }
    return offscreenCanvas;
}

export function receiveCanvas(canvas: OffscreenCanvas) {
    if (offscreenCanvas !== null) {
        throw new Error("Worker already owned a canvas.");
    }
    offscreenCanvas = canvas;
}

export type EngineMessageData = string | OffscreenCanvas | ThreadWorkerInit;

export interface ThreadWorkerInit {
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    name: string,
    callbackPtr: number,
    data: any,
}

export interface EngineWorkerMessage {
    messageType: EngineWorkerMessageType;
    data?: EngineMessageData;
}