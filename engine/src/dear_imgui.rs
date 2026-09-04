use crate::WindowState;
use crate::graphics::ActiveBackend;
use bevy_app::{App, Plugin, PreUpdate, Update};
use bevy_ecs::system::NonSendMut;
use dear_imgui_rs::{BackendFlags, Condition, FrameSnapshot, FrameToken};
use sourcerenderer_core::platform::Window;
use std::cell::{RefCell, RefMut};
use std::marker::PhantomPinned;
use std::pin::Pin;

pub fn install(app: &mut App, window: &impl Window<ActiveBackend>) {
    app.insert_non_send(DearImgui::new(window.width(), window.height()));
    app.add_systems(PreUpdate, (begin_frame_system,));
    app.add_systems(Update, (test_ui_system,));
}

struct FrameWithRef {
    _context_borrow: RefMut<'static, dear_imgui_rs::Context>,
    frame: FrameToken<'static>,
}

impl<'a> AsRef<dear_imgui_rs::FrameToken<'a>> for FrameWithRef {
    fn as_ref(&self) -> &FrameToken<'a> {
        &self.frame
    }
}

struct NoUnpinContext {
    context: RefCell<dear_imgui_rs::Context>,
    _no_unpin: PhantomPinned,
}

pub struct DearImgui {
    frame: Option<FrameWithRef>,
    context_wrapper: Pin<Box<NoUnpinContext>>,
    consumer: dear_imgui_rs::DetachedRendererConsumer,
}

impl Default for DearImgui {
    fn default() -> Self {
        Self::new(1024, 768)
    }
}

impl DearImgui {
    fn new(width: u32, height: u32) -> Self {
        let mut context = dear_imgui_rs::Context::create();
        context
            .set_platform_name(Some("Dreieck".to_string()))
            .unwrap();

        let io = context.io_mut();
        io.set_display_size([width as f32, height as f32]);
        io.set_backend_flags(BackendFlags::RENDERER_HAS_TEXTURES);

        let consumer = context.create_detached_renderer_consumer().unwrap();

        let boxed = Box::pin(NoUnpinContext {
            context: RefCell::new(context),
            _no_unpin: PhantomPinned,
        });
        Self {
            frame: None,
            context_wrapper: boxed,
            consumer,
        }
    }

    fn begin_frame(&mut self) {
        assert!(self.frame.is_none());

        let mut context_ref = self.context_wrapper.context.borrow_mut();
        let frame = context_ref.begin_frame();

        // SAFETY: refcell avoids multiple mutable accesses, the frame token is only alive for
        // as long as the context is and the unpin wrapper + Pin<Box<Context>>> prevents moves.
        let frame_wrapper = unsafe {
            let frame_static: FrameToken<'static> = std::mem::transmute(frame);
            let context_ref_static: RefMut<'static, dear_imgui_rs::Context> =
                std::mem::transmute(context_ref);
            FrameWithRef {
                frame: frame_static,
                _context_borrow: context_ref_static,
            }
        };

        self.frame.replace(frame_wrapper);
    }

    pub fn ui(&self) -> &dear_imgui_rs::Ui {
        self.frame.as_ref().expect("No active frame").frame.ui()
    }

    pub fn draw_data(&mut self) -> Option<FrameSnapshot> {
        let snapshot = {
            let frame = self.frame.take()?.frame;
            frame.render_snapshot(&self.consumer).ok()
        };
        self.context_wrapper.context.borrow_mut().end_frame();
        snapshot
    }

    pub fn window_changed(&mut self, window_state: WindowState) {
        let mut context = self.context_wrapper.context.borrow_mut();
        match window_state {
            WindowState::Window(width, height) | WindowState::Fullscreen(width, height) => {
                context
                    .io_mut()
                    .set_display_size([width as f32, height as f32]);
            }
            _ => {}
        }
    }
}

fn begin_frame_system(mut imgui: NonSendMut<DearImgui>) {
    imgui.begin_frame();
}

fn test_ui_system(mut imgui: NonSendMut<DearImgui>) {
    let ui = imgui.ui();
    ui.window("Hello World")
        .size([300.0, 100.0], Condition::FirstUseEver)
        .build(|| {
            ui.text("Hello, world!");
            ui.text("This is Dear ImGui with docking support!");
        });
}
