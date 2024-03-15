use std::sync::Arc;
use sourcerenderer_core::gpu::GPUBackend;

use crate::graphics::BufferSlice;

use super::drawable::View;
use crate::input::InputState;

pub trait LateLatching<B: GPUBackend>: Send + Sync {
    fn buffer(&self) -> Arc<BufferSlice<B>>;
    fn history_buffer(&self) -> Option<Arc<BufferSlice<B>>>;
    fn before_recording(&self, input: &InputState, view: &View);
    fn before_submit(&self, input: &InputState, view: &View);
    fn after_submit(&self, device: &crate::graphics::Device<B>);
}
