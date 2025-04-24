use bevy_app::App;
use sourcerenderer_core::platform::{
    GraphicsPlatform,
    Window,
};

use super::{
    active_gpu_backend,
    APIInstance,
    Surface,
};

pub struct GPUInstanceResource(pub APIInstance);

pub struct GPUSurfaceResource {
    pub surface: Surface,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn initialize_graphics<P: GraphicsPlatform<active_gpu_backend::Backend>>(
    app: &mut App,
    window: &impl Window<active_gpu_backend::Backend>,
) {
    let api_instance = P::create_instance(false).expect("Failed to initialize graphics");

    let gpu_surface = window.create_surface(&api_instance);
    app.insert_non_send_resource(GPUSurfaceResource {
        surface: gpu_surface,
        width: window.width(),
        height: window.height(),
    });
    app.insert_non_send_resource(GPUInstanceResource(api_instance));
}
