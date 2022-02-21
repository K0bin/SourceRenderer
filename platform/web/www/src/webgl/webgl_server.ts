class WebGLPipeline {
  //private program: WebGLProgram
}

class WebGLServer {
  private textures: Map<Number, WebGLTexture> = new Map();
  private shaders: Map<Number, WebGLShader> = new Map();
  private buffers: Map<Number, WebGLServerBuffer> = new Map();
  private pipelines: Map<Number, WebGLPipeline> = new Map();

  private context: WebGL2RenderingContext;

  public constructor(canvas: HTMLCanvasElement) {
    this.context = canvas.getContext("webgl2", {
      antialias: false
    }) as WebGL2RenderingContext;
  }

  public tryExecute(command: any): boolean {
    if (command.commandType != WEBGL_COMMAND_TYPE) {
      return false;
    }

    switch (command.commandId) {
      case WebGLCreateBufferCommand.COMMAND_ID: {
        const cmd = command as WebGLCreateBufferCommand;
        this.buffers.set(cmd.id, new WebGLServerBuffer(this.context, cmd));
      } break;
      case WebGLSetBufferDataCommand.COMMAND_ID: {
        const cmd = command as WebGLSetBufferDataCommand;
        this.buffers.get(cmd.id)!.setBufferData(this.context, cmd);
      } break;
      case WebGLDestroyBufferDataCommand.COMMAND_ID: {
        const cmd = command as WebGLDestroyBufferDataCommand;
        this.buffers.delete(cmd.id);
      } break;
    }

    return false;
  }
}

class WebGLServerBuffer {
  private readonly buffer: WebGLBuffer;
  private readonly size: number;
  private readonly isIndexBuffer: boolean;

  public constructor(context: WebGL2RenderingContext, cmd: WebGLCreateBufferCommand) {
    this.size = cmd.size;
    this.buffer = context.createBuffer()!;
    this.isIndexBuffer = false; // TODO
  }

  public setBufferData(context: WebGL2RenderingContext, cmd: WebGLSetBufferDataCommand) {
    const target = this.isIndexBuffer ? context.ELEMENT_ARRAY_BUFFER : context.ARRAY_BUFFER;
    context.bindBuffer(target, this.buffer);
    context.bufferData(target, Math.min(cmd.memoryView.byteLength, this.size), context.STATIC_DRAW); // TODO: figure out buffer usage
  }
}
