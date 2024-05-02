pub fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    if value == 0 {
        return 0;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub fn align_down(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}

pub fn align_up_32(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    if value == 0 {
        return 0;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub fn align_down_32(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}

pub fn align_up_64(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

pub fn align_down_64(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    (value / alignment) * alignment
}
