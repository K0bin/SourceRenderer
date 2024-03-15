#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[repr(C, align(8))]
pub struct FfxFsr2Context {
    pub data: [u32; FFX_FSR2_CONTEXT_SIZE as usize]
}
