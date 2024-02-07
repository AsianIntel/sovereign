use std::{error::Error, mem::ManuallyDrop};
use windows::{
    core::ComInterface,
    Win32::{
        Foundation::RECT,
        Graphics::{Direct3D::D3D_PRIMITIVE_TOPOLOGY, Direct3D12::*},
    },
};

pub struct CommandEncoder {
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
}

impl CommandEncoder {
    pub fn new(allocator: ID3D12CommandAllocator, list: ID3D12GraphicsCommandList) -> Self {
        Self { allocator, list }
    }

    pub fn reset(&self) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.allocator.Reset()?;
            self.list.Reset(&self.allocator, None)?;
        }

        Ok(())
    }

    pub fn set_root_signature(&self, root_signature: &ID3D12RootSignature) {
        unsafe {
            self.list.SetGraphicsRootSignature(root_signature);
        }
    }

    pub fn set_viewport(&self, width: u32, height: u32) {
        let view = D3D12_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: width as f32,
            Height: height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        unsafe {
            self.list.RSSetViewports(&[view]);
        }
    }

    pub fn set_scissor(&self, width: u32, height: u32) {
        let scissor = RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        unsafe {
            self.list.RSSetScissorRects(&[scissor]);
        }
    }

    pub fn transition_image(
        &self,
        resource: &ID3D12Resource,
        state_before: D3D12_RESOURCE_STATES,
        state_after: D3D12_RESOURCE_STATES,
    ) {
        let barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: unsafe { std::mem::transmute_copy(resource) },
                    StateBefore: state_before,
                    StateAfter: state_after,
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                }),
            },
        };
        unsafe {
            self.list.ResourceBarrier(&[barrier]);
        }
    }

    pub fn set_render_target(
        &self,
        render_target: D3D12_CPU_DESCRIPTOR_HANDLE,
        depth_target: Option<*const D3D12_CPU_DESCRIPTOR_HANDLE>,
    ) {
        unsafe {
            self.list
                .OMSetRenderTargets(1, Some(&render_target), false, depth_target);
        }
    }

    pub fn clear_render_target(
        &self,
        render_target: D3D12_CPU_DESCRIPTOR_HANDLE,
        clear_color: &[f32; 4],
    ) {
        unsafe {
            self.list
                .ClearRenderTargetView(render_target, clear_color, None);
        }
    }

    pub fn clear_depth_target(&self, depth_target: D3D12_CPU_DESCRIPTOR_HANDLE, depth: f32) {
        unsafe {
            self.list
                .ClearDepthStencilView(depth_target, D3D12_CLEAR_FLAG_DEPTH, depth, 0, &[]);
        }
    }

    pub fn set_primitive_topology(&self, topology: D3D_PRIMITIVE_TOPOLOGY) {
        unsafe {
            self.list.IASetPrimitiveTopology(topology);
        }
    }

    pub fn finish(&self) -> Result<ID3D12CommandList, Box<dyn Error>> {
        unsafe {
            self.list.Close()?;
        }

        Ok(self.list.cast()?)
    }
}
