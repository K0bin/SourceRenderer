
use std::{collections::HashMap, sync::Arc, cell::{RefCell, Ref}};

use sourcerenderer_core::graphics::{Backend, BarrierSync, BarrierAccess, TextureLayout, CommandBuffer, Barrier, TextureDepthStencilViewInfo, TextureSamplingViewInfo, TextureRenderTargetViewInfo, TextureStorageViewInfo, Device, TextureInfo, BufferInfo, MemoryUsage};

struct AB<T> {
  a: T,
  b: Option<T>
}

struct TrackedTexture<B: Backend> {
  stages: BarrierSync,
  access: BarrierAccess,
  layout: TextureLayout,
  texture: Arc<B::Texture>,
  srvs: HashMap<TextureSamplingViewInfo, Arc<B::TextureSamplingView>>,
  dsvs: HashMap<TextureDepthStencilViewInfo, Arc<B::TextureDepthStencilView>>,
  rtvs: HashMap<TextureRenderTargetViewInfo, Arc<B::TextureRenderTargetView>>,
  uavs: HashMap<TextureStorageViewInfo, Arc<B::TextureStorageView>>,
}

struct TrackedBuffer<B: Backend> {
  stages: BarrierSync,
  access: BarrierAccess,
  buffer: Arc<B::Buffer>
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ABEntry {
  A,
  B
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum HistoryResourceEntry {
  Current,
  Past
}

#[derive(Debug)]
struct GlobalMemoryBarrier {
  stages: BarrierSync,
  access: BarrierAccess
}

const USE_GLOBAL_MEMORY_BARRIERS_FOR_BUFFERS: bool = false;
const USE_COARSE_BARRIERS_FOR_TEXTURES: bool = false;
const USE_COARSE_BARRIERS_FOR_BUFFERS: bool = false;

pub struct RendererResources<B: Backend> {
  device: Arc<B::Device>,
  textures: HashMap<String, AB<RefCell<TrackedTexture<B>>>>,
  buffers: HashMap<String, AB<RefCell<TrackedBuffer<B>>>>,
  current_pass: ABEntry,
  global: RefCell<GlobalMemoryBarrier>
}

impl<B: Backend> RendererResources<B> {
  pub fn new(device: &Arc<B::Device>) -> Self {
    Self {
      device: device.clone(),
      textures: HashMap::new(),
      buffers: HashMap::new(),
      current_pass: ABEntry::A,
      global: RefCell::new(GlobalMemoryBarrier {
        stages: BarrierSync::empty(),
        access: BarrierAccess::empty()
      })
    }
  }

  pub fn swap_history_resources(&mut self) {
    self.current_pass = match self.current_pass {
      ABEntry::A => ABEntry::B,
      ABEntry::B => ABEntry::A
    };
  }

  pub fn create_texture(&mut self, name: &str, info: &TextureInfo, has_history: bool) {
    self.textures.insert(name.to_string(), AB {
      a: RefCell::new(TrackedTexture {
        stages: BarrierSync::empty(),
        access: BarrierAccess::empty(),
        layout: TextureLayout::Undefined,
        texture: self.device.create_texture(info, Some(name)),
        srvs: HashMap::new(),
        uavs: HashMap::new(),
        dsvs: HashMap::new(),
        rtvs: HashMap::new()
      }),
      b: has_history.then(|| RefCell::new(TrackedTexture {
        stages: BarrierSync::empty(),
        access: BarrierAccess::empty(),
        layout: TextureLayout::Undefined,
        texture: self.device.create_texture(info, Some(&(name.to_string() + "_b"))),
        srvs: HashMap::new(),
        uavs: HashMap::new(),
        dsvs: HashMap::new(),
        rtvs: HashMap::new()
      }))
    });
  }

  pub fn create_buffer(&mut self, name: &str, info: &BufferInfo, memory_usage: MemoryUsage, has_history: bool) {
    self.buffers.insert(name.to_string(), AB {
      a: RefCell::new(TrackedBuffer {
        stages: BarrierSync::empty(),
        access: BarrierAccess::empty(),
        buffer: self.device.create_buffer(info, memory_usage, Some(name))
      }),
      b: has_history.then(|| RefCell::new(TrackedBuffer {
        stages: BarrierSync::empty(),
        access: BarrierAccess::empty(),
        buffer: self.device.create_buffer(info, memory_usage, Some(&(name.to_string() + "_b")))
      }))
    });
  }

  fn access_texture_internal(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, mut stages: BarrierSync, mut access: BarrierAccess, layout: TextureLayout, discard: bool, history: HistoryResourceEntry) {
    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());

    if USE_COARSE_BARRIERS_FOR_TEXTURES && !access.is_write() {
      // we're doing a read access
      // use broad scope of stages & access flags to avoid further unnecessary reading barriers
      let all_graphics_shaders: BarrierSync = BarrierSync::VERTEX_SHADER | BarrierSync::FRAGMENT_SHADER ;
      if stages.intersects(all_graphics_shaders) {
        stages |= all_graphics_shaders;
      }
      access = BarrierAccess::SHADER_READ;
    }

    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();

    let mut texture_mut = if !use_b_resource {
      texture_ab.a.borrow_mut()
    } else {
      texture_ab.b.as_ref().unwrap().borrow_mut()
    };
    let needs_barrier = access.is_write() || texture_mut.access.is_write() || texture_mut.layout != layout || !texture_mut.access.contains(access) || !texture_mut.stages.contains(stages);
    if needs_barrier {
      cmd_buffer.barrier(&[
        Barrier::TextureBarrier {
          old_sync: texture_mut.stages,
          new_sync: stages,
          old_layout: if !discard { texture_mut.layout } else { TextureLayout::Undefined },
          new_layout: layout,
          old_access: if !discard { texture_mut.access & BarrierAccess::write_mask() } else { BarrierAccess::empty() },
          new_access: access,
          texture: &texture_mut.texture,
        }
      ]);
      if access.is_write() || texture_mut.layout != layout {
        texture_mut.access = access;
      } else {
        texture_mut.access |= access;
      }
      texture_mut.stages = stages;
      texture_mut.layout = layout;
    }
  }

  pub fn access_texture(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, stages: BarrierSync, access: BarrierAccess, layout: TextureLayout, discard: bool, history: HistoryResourceEntry) -> Ref<Arc<B::Texture>> {
    self.access_texture_internal(cmd_buffer, name, stages, access, layout, discard, history);
    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();
    let texture_ref = if !use_b_resource {
      texture_ab.a.borrow()
    } else {
      texture_ab.b.as_ref().unwrap().borrow()
    };
    Ref::map(texture_ref, |r| &r.texture)
  }

  pub fn access_srv(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, stages: BarrierSync, access: BarrierAccess, layout: TextureLayout, discard: bool, info: &TextureSamplingViewInfo, history: HistoryResourceEntry) -> Ref<Arc<B::TextureSamplingView>> {
    debug_assert_eq!(layout, TextureLayout::Sampled);
    debug_assert_eq!(access & !(BarrierAccess::SAMPLING_READ | BarrierAccess::SHADER_READ), BarrierAccess::empty());
    debug_assert_eq!(stages & !(BarrierSync::COMPUTE_SHADER | BarrierSync::FRAGMENT_SHADER | BarrierSync::VERTEX_SHADER | BarrierSync::RAY_TRACING), BarrierSync::empty());
    self.access_texture_internal(cmd_buffer, name, stages, access, layout, discard, history);

    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();
    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      if texture_ref.srvs.contains_key(info) {
        return Ref::map(texture_ref, |r| r.srvs.get(info).unwrap());
      }
    }

    {
      let mut texture_mut = if !use_b_resource {
        texture_ab.a.borrow_mut()
      } else {
        texture_ab.b.as_ref().unwrap().borrow_mut()
      };
      let view = self.device.create_sampling_view(&texture_mut.texture, info, Some(&(name.to_string() + "_srv")));
      texture_mut.srvs.insert(info.clone(), view);
    }

    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      return Ref::map(texture_ref, |r| r.srvs.get(info).unwrap());
    }
  }

  pub fn access_uav(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, stages: BarrierSync, access: BarrierAccess, layout: TextureLayout, discard: bool, info: &TextureStorageViewInfo, history: HistoryResourceEntry) -> Ref<Arc<B::TextureStorageView>> {
    debug_assert_eq!(layout, TextureLayout::Storage);
    debug_assert_eq!(access & !(BarrierAccess::SHADER_READ | BarrierAccess::SHADER_WRITE | BarrierAccess::STORAGE_READ | BarrierAccess::STORAGE_WRITE), BarrierAccess::empty());
    debug_assert_eq!(stages & !(BarrierSync::COMPUTE_SHADER | BarrierSync::FRAGMENT_SHADER | BarrierSync::VERTEX_SHADER | BarrierSync::RAY_TRACING), BarrierSync::empty());
    self.access_texture_internal(cmd_buffer, name, stages, access, layout, discard, history);

    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();
    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      if texture_ref.uavs.contains_key(info) {
        return Ref::map(texture_ref, |r| r.uavs.get(info).unwrap());
      }
    }

    {
      let mut texture_mut = if !use_b_resource {
        texture_ab.a.borrow_mut()
      } else {
        texture_ab.b.as_ref().unwrap().borrow_mut()
      };
      let view = self.device.create_storage_view(&texture_mut.texture, info, Some(&(name.to_string() + "_uav")));
      texture_mut.uavs.insert(info.clone(), view);
    }

    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      return Ref::map(texture_ref, |r| r.uavs.get(info).unwrap());
    }
  }

  pub fn access_rtv(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, stages: BarrierSync, access: BarrierAccess, layout: TextureLayout, discard: bool, info: &TextureRenderTargetViewInfo, history: HistoryResourceEntry) -> Ref<Arc<B::TextureRenderTargetView>> {
    debug_assert_eq!(layout, TextureLayout::RenderTarget);
    debug_assert_eq!(access & !(BarrierAccess::RENDER_TARGET_READ | BarrierAccess::RENDER_TARGET_WRITE), BarrierAccess::empty());
    debug_assert_eq!(stages & !(BarrierSync::RENDER_TARGET), BarrierSync::empty());
    self.access_texture_internal(cmd_buffer, name, stages, access, layout, discard, history);

    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();
    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      if texture_ref.rtvs.contains_key(info) {
        return Ref::map(texture_ref, |r| r.rtvs.get(info).unwrap());
      }
    }

    {
      let mut texture_mut = if !use_b_resource {
        texture_ab.a.borrow_mut()
      } else {
        texture_ab.b.as_ref().unwrap().borrow_mut()
      };
      let view = self.device.create_render_target_view(&texture_mut.texture, info, Some(&(name.to_string() + "_rtv")));
      texture_mut.rtvs.insert(info.clone(), view);
    }

    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      return Ref::map(texture_ref, |r| r.rtvs.get(info).unwrap());
    }
  }

  pub fn access_dsv(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, stages: BarrierSync, access: BarrierAccess, layout: TextureLayout, discard: bool, info: &TextureDepthStencilViewInfo, history: HistoryResourceEntry) -> Ref<Arc<B::TextureDepthStencilView>> {
    debug_assert!(layout == TextureLayout::DepthStencilRead || layout == TextureLayout::DepthStencilReadWrite);
    debug_assert_eq!(access & !(BarrierAccess::DEPTH_STENCIL_READ | BarrierAccess::DEPTH_STENCIL_WRITE), BarrierAccess::empty());
    debug_assert_eq!(stages & !(BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH), BarrierSync::empty());
    self.access_texture_internal(cmd_buffer, name, stages, access, layout, discard, history);

    let texture_ab = self.textures.get(name).unwrap_or_else(|| panic!("No tracked texture by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || texture_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && texture_ab.b.is_some();
    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      if texture_ref.dsvs.contains_key(info) {
        return Ref::map(texture_ref, |r| r.dsvs.get(info).unwrap());
      }
    }

    {
      let mut texture_mut = if !use_b_resource {
        texture_ab.a.borrow_mut()
      } else {
        texture_ab.b.as_ref().unwrap().borrow_mut()
      };
      let view = self.device.create_depth_stencil_view(&texture_mut.texture, info, Some(&(name.to_string() + "_dsv")));
      texture_mut.dsvs.insert(info.clone(), view);
    }

    {
      let texture_ref = if !use_b_resource {
        texture_ab.a.borrow()
      } else {
        texture_ab.b.as_ref().unwrap().borrow()
      };
      return Ref::map(texture_ref, |r| r.dsvs.get(info).unwrap());
    }
  }

  pub fn access_buffer(&self, cmd_buffer: &mut B::CommandBuffer, name: &str, mut stages: BarrierSync, mut access: BarrierAccess, history: HistoryResourceEntry) -> Ref<Arc<B::Buffer>> {
    debug_assert_eq!(access & !(BarrierAccess::VERTEX_INPUT_READ | BarrierAccess::INDEX_READ | BarrierAccess::INDIRECT_READ
      | BarrierAccess::CONSTANT_READ | BarrierAccess::COPY_READ | BarrierAccess::COPY_WRITE | BarrierAccess::STORAGE_READ
      | BarrierAccess::STORAGE_WRITE | BarrierAccess::ACCELERATION_STRUCTURE_READ | BarrierAccess::ACCELERATION_STRUCTURE_WRITE
      | BarrierAccess::SHADER_READ | BarrierAccess::SHADER_WRITE | BarrierAccess::MEMORY_READ | BarrierAccess::MEMORY_WRITE
      | BarrierAccess::HOST_READ | BarrierAccess::HOST_WRITE), BarrierAccess::empty());
    debug_assert_eq!(stages & !(BarrierSync::COPY | BarrierSync::VERTEX_INPUT | BarrierSync::VERTEX_SHADER | BarrierSync::FRAGMENT_SHADER
      | BarrierSync::COMPUTE_SHADER | BarrierSync::INDEX_INPUT | BarrierSync::INDIRECT | BarrierSync::ACCELERATION_STRUCTURE_BUILD | BarrierSync::RAY_TRACING), BarrierSync::empty());

    if USE_COARSE_BARRIERS_FOR_BUFFERS && !access.is_write() {
      // we're doing a read access
      // use broad scope of stages & access flags to avoid further unnecessary reading barriers
      let all_graphics: BarrierSync = BarrierSync::EARLY_DEPTH | BarrierSync::LATE_DEPTH | BarrierSync::VERTEX_INPUT | BarrierSync::VERTEX_SHADER | BarrierSync::FRAGMENT_SHADER | BarrierSync::RENDER_TARGET | BarrierSync::INDIRECT;
      if stages.intersects(all_graphics) {
        stages |= all_graphics;
      }
      access = BarrierAccess::MEMORY_READ;
    }

    let buffer_ab = self.buffers.get(name).unwrap_or_else(|| panic!("No tracked buffer by the name {}", name));
    debug_assert!(history != HistoryResourceEntry::Past || buffer_ab.b.is_some());
    let use_b_resource = (history == HistoryResourceEntry::Past) == (self.current_pass == ABEntry::A) && buffer_ab.b.is_some();

    if !USE_GLOBAL_MEMORY_BARRIERS_FOR_BUFFERS {
      let mut buffer_mut = if !use_b_resource {
        buffer_ab.a.borrow_mut()
      } else {
        buffer_ab.b.as_ref().unwrap().borrow_mut()
      };

      let needs_barrier = access.is_write() || buffer_mut.access.is_write() || !buffer_mut.access.contains(access) || !buffer_mut.stages.contains(stages);
      if needs_barrier {
        cmd_buffer.barrier(&[
          Barrier::BufferBarrier {
            old_sync: buffer_mut.stages,
            new_sync: stages,
            old_access: buffer_mut.access & BarrierAccess::write_mask(),
            new_access: access,
            buffer: &buffer_mut.buffer,
          }
        ]);
        if access.is_write() {
          buffer_mut.access = access;
        } else {
          buffer_mut.access |= access;
        }
        buffer_mut.stages = stages;
      }
    } else {
      let mut global_mut = self.global.borrow_mut();
      let needs_barrier = access.is_write() || global_mut.access.is_write() || !global_mut.access.contains(access) || !global_mut.stages.contains(stages);
      if needs_barrier {
        cmd_buffer.barrier(&[
          Barrier::GlobalBarrier {
            old_sync: global_mut.stages,
            new_sync: stages,
            old_access: global_mut.access & BarrierAccess::write_mask(),
            new_access: access
          }
        ]);
        if access.is_write() {
          global_mut.access = access;
        } else {
          global_mut.access |= access;
        }
        global_mut.stages = stages;
      }
    }

    let buffer_ref = if !use_b_resource {
      buffer_ab.a.borrow()
    } else {
      buffer_ab.b.as_ref().unwrap().borrow()
    };
    Ref::map(buffer_ref, |r| &r.buffer)
  }
}
