pub use lump_data::LumpData;

pub struct Lump {
    pub file_offset: i32,
    pub file_length: i32,
    pub version: i32,
    pub four_cc: i32,
    pub data: LumpData,
}