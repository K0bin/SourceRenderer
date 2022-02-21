const WEBGL_COMMAND_TYPE: string = "gl";

class WebGLCreateBufferCommand {
  public readonly commandType: string = WEBGL_COMMAND_TYPE;
  public readonly commandId: number = WebGLCreateBufferCommand.COMMAND_ID;
  public static readonly COMMAND_ID: number = 0;

  public readonly id: number;
  public readonly size: number;

  public constructor(id: number, size: number) {
    this.id = id;
    this.size = size;
  }
}

class WebGLSetBufferDataCommand {
  public readonly commandType: string = WEBGL_COMMAND_TYPE;
  public readonly commandId: number = WebGLSetBufferDataCommand.COMMAND_ID;
  public static readonly COMMAND_ID: number = 1;

  public readonly id: number;
  public readonly memoryView: DataView;

  public constructor(id: number, memoryView: DataView) {
    this.id = id;
    this.memoryView = memoryView;
  }
}

class WebGLDestroyBufferDataCommand {
  public readonly commandType: string = WEBGL_COMMAND_TYPE;
  public readonly commandId: number = WebGLSetBufferDataCommand.COMMAND_ID;
  public static readonly COMMAND_ID: number = 2;

  public readonly id: number;

  public constructor(id: number) {
    this.id = id;
  }
}

const INPUT_RATE_PER_VERTEX: number = 0;
const INPUT_RATE_PER_INSTANCE: number = 1;

class InputAssemblerElement {
  public readonly binding: number;
  public readonly inputRate: number;
  public readonly stride: number;

  public constructor(binding: number, inputRate: number, stride: number) {
    this.binding = binding;
    this.inputRate = inputRate;
    this.stride = stride;
  }
}

class WebGLCreatePipelineCommand {
  public readonly commandType: string = WEBGL_COMMAND_TYPE;
  public readonly commandId: number = WebGLCreatePipelineCommand.COMMAND_ID;

  public static readonly COMMAND_ID: number = 3;

  public readonly vertexShader: string;
  public readonly fragmentShader: string;

  public constructor(vertexShader: string, fragmentShader: string) {
    this.vertexShader = vertexShader;
    this.fragmentShader = fragmentShader;
  }
}
