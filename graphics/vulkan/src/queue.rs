use ash::vk;

pub struct QueueFamily {
  pub queue_family_index: u32,
  pub queue_count: u32,
  pub queue_priorities: Vec<f32>
}

pub struct QueueDesc {
  pub queue_family_index: u32,
  pub queue_index: u32
}

pub struct Queue {
  queue: vk::Queue,
  desc: QueueDesc
}

impl Queue {
  pub fn new(queue: vk::Queue, desc: QueueDesc) -> Queue {
    return Queue {
      queue: queue,
      desc: desc
    };
  }
}