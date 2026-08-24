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

export type EngineMessageData = string | FakeCanvasData | OffscreenCanvas | ThreadWorkerInit;

export interface FakeCanvasData {
    width: number,
    height: number
}

export interface ThreadWorkerInit {
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    name: string,
    callbackPtr: bigint,
    data: any,
}

export interface EngineWorkerMessage {
    messageType: EngineWorkerMessageType;
    data?: EngineMessageData;
}