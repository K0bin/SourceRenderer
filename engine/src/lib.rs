#![allow(dead_code)]
// extern crate num_cpus;
extern crate crossbeam_channel;
extern crate crossbeam_utils;
extern crate image;
extern crate sourcerenderer_bsp;
extern crate sourcerenderer_core;
extern crate sourcerenderer_mdl;
extern crate sourcerenderer_vmt;
extern crate sourcerenderer_vpk;
extern crate sourcerenderer_vtf;
extern crate sourcerenderer_vtx;
extern crate sourcerenderer_vvd;
extern crate bitset_core;
extern crate bitvec;
extern crate gltf;
extern crate web_time;
extern crate rand;
extern crate rayon;
extern crate regex;
extern crate smallvec;

pub use camera::{
    ActiveCamera,
    Camera,
};

pub use self::engine::Engine;
pub use self::engine::WindowState;

mod engine;

pub mod asset;
pub mod camera;
pub mod fps_camera;
pub mod math;
mod spinning_cube;
pub mod transform;

mod input;
//mod physics;
pub mod renderer;
mod ui;
mod graphics;
