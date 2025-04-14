export enum EngineWorkerMessageType {
    StartThreadFromMain, // Browsers are buggy when starting workers from other workers
    TransferCanvas,
    TransferFakeCanvas,
}

export type EngineMessageData = string|FakeCanvasData|OffscreenCanvas|ThreadWorkerInit;

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
    data: EngineMessageData;
}