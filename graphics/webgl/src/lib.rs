mod backend;
mod instance;
mod device;
mod surface;
mod command;
mod texture;
mod buffer;
mod pipeline;
mod sync;
mod raw_context;
mod thread;
extern crate crossbeam_channel;

pub use backend::WebGLBackend;
use crossbeam_channel::{Receiver, Sender};
pub use instance::{WebGLInstance, WebGLAdapter};
pub use device::WebGLDevice;
pub use surface::{WebGLSurface, WebGLSwapchain};
pub use command::{WebGLCommandBuffer, WebGLCommandSubmission};
pub use texture::{WebGLTexture, WebGLTextureShaderResourceView};
pub(crate) use texture::{format_to_internal_gl, address_mode_to_gl, max_filter_to_gl, min_filter_to_gl};
pub use buffer::WebGLBuffer;
pub use pipeline::{WebGLShader, WebGLGraphicsPipeline, WebGLComputePipeline};
pub use sync::WebGLFence;
pub(crate) use raw_context::RawWebGLContext;

pub use thread::WebGLThreadDevice;

pub type GLThreadSender = Sender<Box<dyn FnOnce(&mut crate::thread::WebGLThreadDevice) + Send>>;
pub type GLThreadReceiver = Receiver<Box<dyn FnOnce(&mut crate::thread::WebGLThreadDevice) + Send>>;
