use std::error::Error;
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandList, ID3D12CommandQueue, ID3D12Fence};

pub struct Queue {
    queue: ID3D12CommandQueue,
}

impl Queue {
    pub fn new(queue: ID3D12CommandQueue) -> Self {
        Self { queue }
    }

    pub fn get(&self) -> &ID3D12CommandQueue {
        &self.queue
    }

    pub fn execute_command_lists(&self, list: &[Option<ID3D12CommandList>]) {
        unsafe {
            self.queue.ExecuteCommandLists(list);
        }
    }

    pub fn signal(&self, fence: &ID3D12Fence, value: u64) -> Result<(), Box<dyn Error>> {
        unsafe { self.queue.Signal(fence, value) }?;
        Ok(())
    }
}
