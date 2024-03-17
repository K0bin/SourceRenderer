use sourcerenderer_core::gpu;

use super::*;

pub enum MTLBackend {}

impl gpu::GPUBackend for MTLBackend {
    type Instance = MTLInstance;
    type Adapter = MTLAdapter;
    type Device = MTLDevice;
    type Buffer = MTLBuffer;
    type Texture = MTLTexture;
    type Sampler = MTLSampler;
    type TextureView = MTLTextureView;
    type Surface = MTLSurface;    
}
