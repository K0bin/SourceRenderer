use std::{sync::Arc, collections::HashMap};

use imgui::{Context, sys::ImDrawCmd, internal::RawWrapper, FontSource, TextureId};
use sourcerenderer_core::{graphics::{Backend, Device, MemoryUsage, BufferUsage, Scissor, Viewport, SampleCount, TextureUsage, Format, TextureDimension, TextureInfo, TextureViewInfo}, Vec2, Vec2UI, Vec2I, Platform};

pub struct UI<P: Platform> {
    imgui: Context,
    texture_map: HashMap<imgui::TextureId, Arc<<P::GraphicsBackend as Backend>::TextureView>>
}

impl<P: Platform> UI<P> {
    pub fn new(device: &Arc<<P::GraphicsBackend as Backend>::Device>) -> Self {
        let mut imgui = imgui::Context::create();
        imgui.set_platform_name(Some("Dreieck".to_string()));
        imgui.style_mut().use_dark_colors();

        let mut texture_map: HashMap<imgui::TextureId, Arc<<P::GraphicsBackend as Backend>::TextureView>> = HashMap::new();

        const FONT_TEXTURE_ID: usize = 1;

        imgui.fonts().add_font(&[FontSource::DefaultFontData { config: None }]);
        let font_tex_data = imgui.fonts().build_rgba32_texture();
        let font_texture = device.create_texture(&TextureInfo {
            dimension: TextureDimension::Dim2D,
            format: Format::RGBA8UNorm,
            width: font_tex_data.width,
            height: font_tex_data.height,
            depth: 1,
            mip_levels: 1,
            array_length: 1,
            samples: SampleCount::Samples1,
            usage: TextureUsage::COPY_DST | TextureUsage::SAMPLED,
            supports_srgb: false,
        }, Some("DearImguiFontMap"));
        let font_data = device.upload_data(font_tex_data.data, MemoryUsage::UncachedRAM, BufferUsage::COPY_SRC);
        device.init_texture(&font_texture, &font_data, 0, 0, 0);
        device.flush_transfers();
        let font_texture_view = device.create_texture_view(&font_texture, &TextureViewInfo::default(), Some("DearImguiFontMapView"));

        imgui.fonts().tex_id = TextureId::new(FONT_TEXTURE_ID);
        texture_map.insert(imgui.fonts().tex_id, font_texture_view);

        Self {
            imgui,
            texture_map
        }
    }

    pub fn update(&mut self) {
        self.imgui.io_mut().display_size = [ 1280f32, 720f32 ];
        self.imgui.io_mut().display_framebuffer_scale = [ 1f32, 1f32 ];
        let frame = self.imgui.frame();
        frame.text("Hi");
        let mut opened = false;
        frame.show_demo_window(&mut opened)
    }

    pub fn draw_data(&mut self, device: &Arc<<P::GraphicsBackend as Backend>::Device>) -> UIDrawData<P::GraphicsBackend> {
        let draw = self.imgui.render();
        let mut draw_lists = Vec::<UICmdList<P::GraphicsBackend>>::with_capacity(draw.draw_lists_count());

        let fb_size = Vec2::new(draw.display_size[0] * draw.framebuffer_scale[0], draw.display_size[1] * draw.framebuffer_scale[1]);
        let scale = Vec2::new(
            2f32 / draw.display_size[0],
            2f32 / draw.display_size[1],
        );
        let translate = Vec2::new(
            -1f32 - draw.display_pos[0] * scale.x,
            -1f32 - draw.display_pos[1] * scale.y,
        );

        let clip_offset = Vec2::new(draw.display_pos[0], draw.display_pos[1]);
        let clip_scale = Vec2::new(draw.framebuffer_scale[0], draw.framebuffer_scale[1]);

        let viewport = Viewport {
            position: Vec2::new(0f32, 0f32),
            extent: fb_size,
            min_depth: 0.0f32,
            max_depth: 1.0f32,
        };

        for list in draw.draw_lists() {
            let vertex_buffer = device.upload_data(list.vtx_buffer(), MemoryUsage::MappableVRAM, BufferUsage::VERTEX);
            let index_buffer = device.upload_data(list.idx_buffer(), MemoryUsage::MappableVRAM, BufferUsage::INDEX);
            let mut draws = Vec::<UIDraw<P::GraphicsBackend>>::new();

            for cmd in list.commands() {
                match cmd {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        let mut clip_min = Vec2::new((cmd_params.clip_rect[0] - clip_offset.x) * clip_scale.x, (cmd_params.clip_rect[1] - clip_offset.y) * clip_scale.y);
                        let mut clip_max = Vec2::new((cmd_params.clip_rect[2] - clip_offset.x) * clip_scale.x, (cmd_params.clip_rect[3] - clip_offset.y) * clip_scale.y);

                        if clip_min.x < 0.0f32 { clip_min.x = 0.0f32; }
                        if clip_min.y < 0.0f32 { clip_min.y = 0.0f32; }
                        if clip_max.x > fb_size.x { clip_max.x = fb_size.y; }
                        if clip_max.y > fb_size.y { clip_max.y = fb_size.y; }
                        if clip_max.x <= clip_min.x || clip_max.y <= clip_min.y { continue; }

                        draws.push(UIDraw {
                            scissor: Scissor {
                                position: Vec2I::new(clip_min.x as i32, clip_min.y as i32),
                                extent: Vec2UI::new((clip_max.x - clip_min.x) as u32, (clip_max.y - clip_min.y) as u32),
                            },
                            texture: self.texture_map.get(&cmd_params.texture_id).cloned(),
                            vertex_offset: cmd_params.vtx_offset as u32,
                            first_index: cmd_params.idx_offset as u32,
                            index_count: count as u32
                        });
                    }
                    imgui::DrawCmd::ResetRenderState => {},
                    imgui::DrawCmd::RawCallback { callback, raw_cmd } => {
                        unsafe {
                            callback(list.raw(), raw_cmd);
                        }
                    }
                }
            }

            draw_lists.push(UICmdList { vertex_buffer, index_buffer, draws });
        }
        return UIDrawData {
            draw_lists,
            viewport,
            scale,
            translate,
        };
    }
}

pub struct UIDrawData<B: Backend> {
    pub draw_lists: Vec<UICmdList<B>>,
    pub viewport: Viewport,
    pub scale: Vec2,
    pub translate: Vec2
}

pub struct UICmdList<B: Backend> {
    pub vertex_buffer: Arc<B::Buffer>,
    pub index_buffer: Arc<B::Buffer>,
    pub draws: Vec<UIDraw<B>>
}

pub struct UIDraw<B: Backend> {
    pub texture: Option<Arc<B::TextureView>>,
    pub vertex_offset: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub scissor: Scissor
}

impl<B: Backend> Default for UIDrawData<B> {
    fn default() -> Self {
        Self {
            draw_lists: Vec::new(),
            viewport: Viewport { position: Vec2::new(0f32, 0f32), extent: Vec2::new(0f32, 0f32), min_depth: 0f32, max_depth: 0f32 },
            scale: Vec2::new(1f32, 1f32),
            translate: Vec2::new(0f32, 0f32)
        }
    }
}