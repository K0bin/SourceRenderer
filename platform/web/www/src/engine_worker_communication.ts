import {InitOutput} from "sourcerenderer_web";

export enum EngineWorkerMessageType {
    StartRenderThread,
    InitMainThread,
    InitThread,
    CanvasResized,
    MouseMoved,
    KeyUp,
    KeyDown,
    UpdateMouseLock
}

export type EngineMessageData =
    string
    | OffscreenCanvas
    | ThreadWorkerInit
    | CanvasResized
    | MouseMoved
    | boolean;

export interface ThreadWorkerInit {
    module: WebAssembly.Module,
    memory: WebAssembly.Memory,
    name: string,
    callbackPtr: number,
    data: any,
}

export interface CanvasResized {
    width: number,
    height: number,
}

export interface MouseMoved {
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
}

export interface EngineWorkerMessage {
    messageType: EngineWorkerMessageType;
    data?: EngineMessageData;
}

export function destroyThread(thread: InitOutput) {
    // This has to be done in a separate function because
    // the Promise -> Rust conversion happens after run is done.
    // It also has to happen inside of JS.
    console.log("Destroying thread");
    thread.__wbindgen_thread_destroy();
    console.log("Thread destroyed");
}
