use std::sync::{Arc, Mutex};

use metal;
use block::ConcreteBlock;

use sourcerenderer_core::gpu;

use super::*;

pub struct MTLAccelerationStructure {}

impl MTLAccelerationStructure {
    pub(crate) fn bottom_level_size(device: &metal::DeviceRef, info: &gpu::BottomLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        unimplemented!()
    }

    pub(crate) fn top_level_size(device: &metal::DeviceRef, info: &gpu::TopLevelAccelerationStructureInfo<MTLBackend>) -> gpu::AccelerationStructureSizes {
        unimplemented!()
    }
}

impl gpu::AccelerationStructure for MTLAccelerationStructure {}
