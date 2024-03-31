use std::sync::{Arc, Mutex};

use smallvec::SmallVec;

use sourcerenderer_core::gpu::ResourceHeapInfo;
use windows::core::GUID;
use windows::Win32::Graphics::Direct3D as D3D;
use windows::Win32::Graphics::Dxgi;
use windows::Win32::Graphics::Direct3D12 as D3D12;
use windows::core::Interface;

use sourcerenderer_core::gpu;

use super::*;

struct SlotAllocatorInner {
    words: SmallVec<[u64; 8]>,
    word_map: u32
}

pub(crate) struct SlotAllocator {
    inner: Arc<Mutex<SlotAllocatorInner>>,
    size: u32,
    offset: u32,

    suballocated_range: Option<SlotRange>
}

impl SlotAllocator {
    fn new_internal(size: u32, suballocated_range: Option<SlotRange>) -> Self {
        assert_eq!(size % 64, 0);
        let mut words = SmallVec::<[u64; 8]>::with_capacity((size as usize) / 64);
        let mut word_map = 0u32;
        for i in 0..(size / 64) {
            words.push(!0u64);
            word_map |= 1 << i;
        }
        Self {
            inner: Arc::new(Mutex::new(SlotAllocatorInner {
                words,
                word_map
            })),
            size,
            offset: suballocated_range.map(|r|r.start).unwrap_or(0),
            suballocated_range: suballocated_range
        }
    }

    pub fn new(size: u32) -> Self {
        Self::new_internal(size, None)
    }

    pub fn new_suballocated(suballocated_range: SlotRange) -> Self {
        Self::new_internal(suballocated_range.length, Some(suballocated_range))
    }

    pub fn alloc(&self) -> Option<Slot> {
        let mut guard = self.inner.lock().unwrap();

        if guard.word_map == 0 {
            return None;
        }

        let word_index = guard.word_map.trailing_zeros();
        let word = &mut guard.words[word_index as usize];
        assert_ne!(*word, 0u64);

        let zeros = word.trailing_zeros();
        *word &= !(1u64 << zeros);

        if *word == 0u64 {
            guard.word_map &= !(1u32 << word_index);
        }

        let slot = word_index * 64 + zeros + self.offset;

        return Some(Slot {
            inner: self.inner.clone(),
            slot
        });
    }

    pub fn alloc_range(&self, len: u32) -> Option<SlotRange> {
        let mut guard = self.inner.lock().unwrap();

        if guard.word_map == 0 {
            return None;
        }
        let mask = (1u64 << len) - 1;

        let mut word_map = guard.word_map;
        let mut word_index = word_map.trailing_zeros();
        while word_map != 0 {
            let mut word = guard.words[word_index as usize];

            if word == 0 {
                word_map &= !(1u32 << word_index);
                word_index = word_map.trailing_zeros();
            }

            let zeros = word.trailing_zeros();
            let shifted_mask = mask << zeros;
            if (word & shifted_mask) == shifted_mask {
                guard.words[word_index as usize] &= !shifted_mask;
                if guard.words[word_index as usize] == 0u64 {
                    guard.word_map &= !(1u32 << word_index);
                }
                return Some(SlotRange {
                    inner: self.inner.clone(),
                    start: word_index * 64 + zeros + self.offset,
                    length: len
                });
            }
            word &= !(1u64 << zeros);
        }
        None
    }
}

pub(crate) struct Slot {
    inner: Arc<Mutex<SlotAllocatorInner>>,
    slot: u32,
}

impl Slot {
    #[inline(always)]
    pub fn slot(&self) -> u32 {
        self.slot
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut guard = self.inner.lock().unwrap();
        let word_index = self.slot / 64;
        let bit_index = self.slot % 64;
        let word = &mut guard.words[word_index as usize];
        *word |= 1u64 << bit_index as u64;
        guard.word_map |= 1u32 << word_index;
    }
}

pub(crate) struct SlotRange {
    inner: Arc<Mutex<SlotAllocatorInner>>,
    start: u32,
    length: u32
}

impl SlotRange {
    #[inline(always)]
    pub fn start(&self) -> u32 {
        self.start
    }
    #[inline(always)]
    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Drop for SlotRange {
    fn drop(&mut self) {
        let mut guard = self.inner.lock().unwrap();
    }
}

pub(crate) struct D3D12DescriptorHeap {
    heap: D3D12::ID3D12DescriptorHeap,
    allocator: SlotAllocator,
    increment: u32,
    start_cpu_handle: D3D12::D3D12_CPU_DESCRIPTOR_HANDLE,
    start_gpu_handle: D3D12::D3D12_GPU_DESCRIPTOR_HANDLE,
}

pub(crate) struct D3D12Descriptor {
    pub(crate) cpu_handle: D3D12::D3D12_CPU_DESCRIPTOR_HANDLE,
    pub(crate) gpu_handle: D3D12::D3D12_GPU_DESCRIPTOR_HANDLE,
}

impl D3D12DescriptorHeap {
    pub(crate) fn new(device: &D3D12::ID3D12Device12, descriptor_type: D3D12::D3D12_DESCRIPTOR_HEAP_TYPE, shader_visible: bool, descriptor_count: u32) -> Self {
        let aligned_descriptor_count = (descriptor_count / 64) * 64;

        let mut descriptor_heap_desc = D3D12::D3D12_DESCRIPTOR_HEAP_DESC {
            Type: descriptor_type,
            NumDescriptors: aligned_descriptor_count,
            Flags: if !shader_visible { D3D12::D3D12_DESCRIPTOR_HEAP_FLAG_NONE } else { D3D12::D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE },
            NodeMask: 0,
        };
        let heap: D3D12::ID3D12DescriptorHeap = unsafe { device.CreateDescriptorHeap(&descriptor_heap_desc) }.expect("Failed to create descriptor heap");
        let allocator = SlotAllocator::new(aligned_descriptor_count);
        let increment = unsafe { device.GetDescriptorHandleIncrementSize(descriptor_type) };
        let cpu_handle = unsafe { heap.GetCPUDescriptorHandleForHeapStart() };
        let gpu_handle = unsafe { heap.GetGPUDescriptorHandleForHeapStart() };
        Self {
            heap,
            allocator,
            increment,
            start_cpu_handle: cpu_handle,
            start_gpu_handle: gpu_handle
        }
    }

    pub(crate) fn get_new_descriptor(&self) -> D3D12Descriptor {
        let slot = self.allocator.alloc().expect("Failed to allocate descriptor slot");
        let cpu_handle = D3D12::D3D12_CPU_DESCRIPTOR_HANDLE { ptr: self.start_cpu_handle.ptr + (self.increment * slot.slot) as usize };
        let gpu_handle = D3D12::D3D12_GPU_DESCRIPTOR_HANDLE { ptr: self.start_gpu_handle.ptr + (self.increment * slot.slot) as u64 };
        D3D12Descriptor {
            cpu_handle, gpu_handle
        }
    }
}
