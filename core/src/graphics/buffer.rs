

bitflags! {
  pub struct BufferUsage: u32 {
    const VERTEX        = 0b1;
    const INDEX         = 0b10;
    const CONSTANT      = 0b100;
    const STORAGE       = 0b1000;
    const INDIRECT      = 0b10000;
    const UNIFORM_TEXEL = 0b100000;
    const STORAGE_TEXEL = 0b1000000;
    const COPY_SRC      = 0b1000000000000000000;
    const COPY_DST      = 0b10000000000000000000;
  }
}

pub trait Buffer {

}
