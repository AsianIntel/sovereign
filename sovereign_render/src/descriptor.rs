use std::sync::Arc;
use windows::Win32::Graphics::Direct3D12::{
    ID3D12DescriptorHeap, ID3D12Device, ID3D12Resource, D3D12_CONSTANT_BUFFER_VIEW_DESC,
    D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_SAMPLER_DESC, D3D12_SHADER_RESOURCE_VIEW_DESC,
};

use crate::{
    device::AllocatedImage,
    id::{SamplerId, ViewId},
};

pub struct DescriptorHeap {
    heap: ID3D12DescriptorHeap,
    device: Arc<ID3D12Device>,
    descriptor_size: u32,
    items: usize,
}

impl DescriptorHeap {
    pub fn new(
        heap: ID3D12DescriptorHeap,
        device: Arc<ID3D12Device>,
        descriptor_size: u32,
    ) -> Self {
        Self {
            heap,
            device,
            descriptor_size,
            items: 0,
        }
    }

    pub fn get(&self) -> ID3D12DescriptorHeap {
        self.heap.clone()
    }

    pub fn get_handle(&self, idx: usize) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() }.ptr
                + idx * self.descriptor_size as usize,
        }
    }

    pub fn create_rtv(&mut self, resource: &ID3D12Resource) {
        let idx = self.items;
        unsafe {
            self.device.CreateRenderTargetView(
                resource,
                None,
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self.heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + idx * self.descriptor_size as usize,
                },
            );
        }
        self.items += 1;
    }

    pub fn create_dsv(&mut self, image: &AllocatedImage) {
        let idx = self.items;
        unsafe {
            self.device.CreateDepthStencilView(
                image.allocation.resource(),
                None,
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self.heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + idx * self.descriptor_size as usize,
                },
            );
        }
        self.items += 1;
    }

    pub fn create_sampler(&mut self, sampler: &D3D12_SAMPLER_DESC) -> SamplerId {
        let idx = self.items;
        unsafe {
            self.device.CreateSampler(
                sampler,
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self.heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + idx * self.descriptor_size as usize,
                },
            );
        }
        self.items += 1;
        SamplerId(idx)
    }

    pub fn create_srv(
        &mut self,
        resource: &ID3D12Resource,
        desc: &D3D12_SHADER_RESOURCE_VIEW_DESC,
    ) -> ViewId {
        let idx = self.items;
        unsafe {
            self.device.CreateShaderResourceView(
                resource,
                Some(desc as *const _),
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self.heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + idx * self.descriptor_size as usize,
                },
            );
        }
        self.items += 1;
        ViewId(idx)
    }

    pub fn create_cbv(&mut self, desc: &D3D12_CONSTANT_BUFFER_VIEW_DESC) -> ViewId {
        let idx = self.items;
        unsafe {
            self.device.CreateConstantBufferView(
                Some(desc as *const _),
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self.heap.GetCPUDescriptorHandleForHeapStart().ptr
                        + idx * self.descriptor_size as usize,
                },
            );
        }
        self.items += 1;
        ViewId(idx)
    }
}
