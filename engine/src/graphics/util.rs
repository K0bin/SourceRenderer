#[allow(unused)]
#[inline(always)]
pub fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down(value: usize, alignment: usize) -> usize {
    value & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_up_32(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down_32(value: u32, alignment: u32) -> u32 {
    value & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_up_64(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down_64(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

#[allow(unused)]
#[inline(always)]
pub fn align_up_non_pot(value: usize, alignment: usize) -> usize {
    align_down_non_pot(value + alignment - 1, alignment)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down_non_pot(value: usize, alignment: usize) -> usize {
    (value / alignment) * alignment
}

#[allow(unused)]
#[inline(always)]
pub fn align_up_non_pot_32(value: u32, alignment: u32) -> u32 {
    align_down_non_pot_32(value + alignment - 1, alignment)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down_non_pot_32(value: u32, alignment: u32) -> u32 {
    (value / alignment) * alignment
}

#[allow(unused)]
#[inline(always)]
pub fn align_up_non_pot_64(value: u64, alignment: u64) -> u64 {
    align_down_non_pot_64(value + alignment - 1, alignment)
}

#[allow(unused)]
#[inline(always)]
pub fn align_down_non_pot_64(value: u64, alignment: u64) -> u64 {
    (value / alignment) * alignment
}
