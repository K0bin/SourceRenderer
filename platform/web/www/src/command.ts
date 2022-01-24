class StartWorkerCommand {
  public readonly commandType: string = StartWorkerCommand.COMMAND_TYPE;
  public static readonly COMMAND_TYPE: string = "init";

  public readonly workerId: number;
  public readonly module: WebAssembly.Module;
  public readonly memory: WebAssembly.Memory;
  public constructor(workerId: number, module: WebAssembly.Module, memory: WebAssembly.Memory) {
    this.workerId = workerId;
    this.module = module;
    this.memory = memory;
  }
}

class WorkerWorkCommand {
  public readonly commandType: string = WorkerWorkCommand.COMMAND_TYPE;
  public static readonly COMMAND_TYPE: string = "work";

  public readonly functionPointer: number;
  public constructor(functionPointer: number) {
    this.functionPointer = functionPointer;
  }
}

class ReturnWorkerCommand {
  public readonly commandType: string = ReturnWorkerCommand.COMMAND_TYPE;
  public static readonly COMMAND_TYPE: string = "return";

  public readonly workerId: number;
  public constructor(workerId: number) {
    this.workerId = workerId;
  }
}