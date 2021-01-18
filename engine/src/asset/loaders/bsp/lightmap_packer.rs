use sourcerenderer_bsp::Lighting;
use sourcerenderer_core::graphics::{Texture, Device, TextureInfo, Format, SampleCount, MemoryUsage, BufferUsage};
use sourcerenderer_core::graphics::Backend as GraphicsBackend;
use std::sync::Arc;
use std::option::Option::Some;

const MARGIN: u32 = 2;
const FREE_MIN: u32 = 3 + MARGIN * 2;

struct Rect {
  x: u32,
  y: u32,
  width: u32,
  height: u32
}

impl Rect {
  fn new(width: u32, height: u32) -> Self {
    Self {
      x: 0, y: 0, width, height
    }
  }

  fn area(&self) -> u32 {
    self.width * self.height
  }
}

pub struct LightmapPacker {
  free_list: Vec<Rect>,
  data: Box<[u32]>,
  lightmap_width: u32,
  lightmap_height: u32,
  used_area: u32
}

impl LightmapPacker {
  pub fn new(lightmap_width: u32, lightmap_height: u32) -> Self {
    assert_ne!(lightmap_width, 0);
    assert_ne!(lightmap_height, 0);
    let texture_rect = Rect::new(lightmap_width, lightmap_height);
    let size = (lightmap_width * lightmap_height) as usize;
    let mut data = Vec::<u32>::with_capacity(size);
    for i in 0..size {
      data.push(0); // this is slow and stupid
    }

    Self {
      free_list: vec![texture_rect],
      data: data.into_boxed_slice(),
      lightmap_width,
      lightmap_height,
      used_area: 0
    }
  }

  fn find_space(&mut self, width: u32, height: u32) -> Option<Rect> {
    let mut new_rect: Option<Rect> = None;
    let mut spot: Option<Rect> = None;
    for rect in &mut self.free_list {
      if rect.width >= width + MARGIN * 2 && rect.height >= height + MARGIN * 2 {
        let target = Rect {
          x: rect.x + MARGIN,
          y: rect.y + MARGIN,
          width,
          height
        };

        if rect.width > rect.height {
          new_rect = Some(Rect {
            x: rect.x,
            y: rect.y + MARGIN * 2 + height,
            width: width + MARGIN * 2,
            height: rect.height - MARGIN * 2 - height
          });

          rect.x += MARGIN * 2 + width;
          rect.width -= MARGIN * 2 + width;
        } else {
          new_rect = Some(Rect {
            x: rect.x + MARGIN * 2 + width,
            y: rect.y,
            width: rect.width - MARGIN * 2 - width,
            height: height + MARGIN * 2
          });

          rect.y += MARGIN * 2 + height;
          rect.height -= MARGIN * 2 + height;
        }

        spot = Some(target);
        break;
      }
    }

    if let Some(new_rect) = new_rect {
      if new_rect.width >= FREE_MIN && new_rect.height >= FREE_MIN {
        self.free_list.push(new_rect);
        self.free_list.sort_by_key(|r| r.area());
      }
    }

    spot
  }

  pub fn add_samples(&mut self, width: u32, height: u32, data: &[Lighting]) -> (u32, u32) {
    assert!((data.len() as u32) >= width * height);
    let rect = self.find_space(width, height).unwrap();
    for y in 0 .. height {
      for x in 0 .. width {
        let i = (x + y * width) as usize;
        let offset = (x + rect.x + (y + rect.y) * self.lightmap_width) as usize;
        let sample = &data[i].color;
        self.data[offset] = sample.to_u32_color();
      }
    }
    self.used_area += width * height;
    (rect.x, rect.y)
  }

  pub fn build_texture<B: GraphicsBackend>(&mut self, device: &Arc<B::Device>) -> Arc<B::Texture> {
    println!("Lightmap used {} texels", self.used_area);

    let texture = device.create_texture(&TextureInfo {
      format: Format::RGBA8,
      width: self.lightmap_width,
      height: self.lightmap_height,
      depth: 1,
      mip_levels: 1,
      array_length: 1,
      samples: SampleCount::Samples1
    }, Some("Lightmap"));
    let buffer = device.upload_data_slice(&self.data, MemoryUsage::CpuToGpu, BufferUsage::COPY_SRC);
    device.init_texture(&texture, &buffer, 0, 0);
    texture
  }

  pub fn texture_width(&self) -> u32 {
    self.lightmap_width
  }

  pub fn texture_height(&self) -> u32 {
    self.lightmap_height
  }
}
