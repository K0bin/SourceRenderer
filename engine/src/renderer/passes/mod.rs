#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod blue_noise;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod clustering;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod compositing;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod conservative;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod light_binning;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod prepass;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod sharpen;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod ssao;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod ssr;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod taa;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod blit;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod path_tracing;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod modern;
#[cfg(not(target_arch = "wasm32"))]
use modern::rt_shadows;

pub(crate) mod web;
pub(crate) mod ui;
