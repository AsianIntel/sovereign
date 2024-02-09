use gpu_allocator::{
    d3d12::{
        Allocator, AllocatorCreateDesc, ID3D12DeviceVersion, Resource, ResourceCategory,
        ResourceCreateDesc, ResourceStateOrBarrierLayout, ResourceType,
    },
    MemoryLocation,
};
use std::{error::Error, ffi::c_void, mem::MaybeUninit, ptr, sync::Arc};
use windows::{
    core::{ComInterface, PCSTR},
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D::*,
            Direct3D12::*,
            Dxgi::{Common::*, *},
        },
    },
};

use crate::{command_encoder::CommandEncoder, descriptor::DescriptorHeap, id::{BufferId, ImageId}, queue::Queue};

pub type DeviceError = Box<dyn Error>;

pub struct Device {
    factory: IDXGIFactory6,
    physical_device: IDXGIAdapter1,
    device: Arc<ID3D12Device>,
    allocator: Allocator,
    debug_callback: ID3D12InfoQueue1,

    images: Vec<AllocatedImage>,
    buffers: Vec<AllocatedBuffer>,
}

pub struct AllocatedImage {
    pub allocation: Resource,
    pub width: u32,
    pub height: u32
}

pub struct AllocatedBuffer {
    pub allocation: Resource,
    pub size: u64
}

impl Device {
    pub fn new() -> Result<Self, DeviceError> {
        if cfg!(debug_assertions) {
            unsafe {
                let mut debug: Option<ID3D12Debug1> = None;
                D3D12GetDebugInterface(&mut debug)?;
                if let Some(debug) = debug {
                    debug.EnableDebugLayer();
                }
            }
        }

        let factory_flags = DXGI_CREATE_FACTORY_DEBUG;
        let factory: IDXGIFactory6 = unsafe { CreateDXGIFactory2(factory_flags) }?;

        let physical_device = get_physical_device(&factory)?;

        let mut device: Option<ID3D12Device> = None;
        unsafe { D3D12CreateDevice(&physical_device, D3D_FEATURE_LEVEL_11_0, &mut device) }?;
        let device = device.unwrap();

        let allocator = Allocator::new(&AllocatorCreateDesc {
            device: ID3D12DeviceVersion::Device(device.clone()),
            debug_settings: Default::default(),
            allocation_sizes: Default::default(),
        })?;

        let mut info_queue: Option<ID3D12InfoQueue1> = None;
        unsafe { device.query(&ID3D12InfoQueue1::IID, &mut info_queue as *mut _ as *mut _) }
            .ok()?;
        let info_queue = info_queue.unwrap();
        let mut ids = vec![D3D12_MESSAGE_ID_CLEARDEPTHSTENCILVIEW_MISMATCHINGCLEARVALUE];
        unsafe {
            info_queue.AddStorageFilterEntries(&D3D12_INFO_QUEUE_FILTER {
                DenyList: D3D12_INFO_QUEUE_FILTER_DESC {
                    NumIDs: 1,
                    pIDList: ids.as_mut_ptr(),
                    ..Default::default()
                },
                ..Default::default()
            })?;
        }
        let mut callback = 0;
        unsafe {
            info_queue.RegisterMessageCallback(
                Some(message_callback),
                D3D12_MESSAGE_CALLBACK_FLAG_NONE,
                ptr::null(),
                &mut callback,
            )
        }?;

        Ok(Self {
            factory,
            physical_device,
            device: Arc::new(device),
            allocator,
            debug_callback: info_queue,
            images: Vec::new(),
            buffers: Vec::new(),
        })
    }

    pub fn get_image(&self, image_id: ImageId) -> &AllocatedImage {
        &self.images[image_id.0]
    }

    pub fn get_buffer(&self, buffer_id: BufferId) -> &AllocatedBuffer {
        &self.buffers[buffer_id.0]
    }

    pub fn create_command_queue(
        &self,
        kind: D3D12_COMMAND_LIST_TYPE,
    ) -> Result<Queue, DeviceError> {
        let desc = D3D12_COMMAND_QUEUE_DESC {
            Type: kind,
            ..Default::default()
        };

        Ok(Queue::new(unsafe {
            self.device.CreateCommandQueue(&desc)?
        }))
    }

    pub fn create_swapchain(
        &self,
        desc: &DXGI_SWAP_CHAIN_DESC1,
        queue: &Queue,
        hwnd: HWND,
    ) -> Result<IDXGISwapChain3, DeviceError> {
        let swapchain: IDXGISwapChain3 = unsafe {
            self.factory
                .CreateSwapChainForHwnd(queue.get(), hwnd, desc, None, None)
        }?
        .cast()?;

        unsafe {
            self.factory
                .MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)?;
        }

        Ok(swapchain)
    }

    pub fn create_descriptor_heap(
        &self,
        kind: D3D12_DESCRIPTOR_HEAP_TYPE,
        count: u32,
        flags: D3D12_DESCRIPTOR_HEAP_FLAGS,
    ) -> Result<DescriptorHeap, DeviceError> {
        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: kind,
            NumDescriptors: count,
            Flags: flags,
            ..Default::default()
        };
        let raw_heap: ID3D12DescriptorHeap = unsafe { self.device.CreateDescriptorHeap(&desc)? };
        let descriptor_size = unsafe { self.device.GetDescriptorHandleIncrementSize(kind) };
        let heap = DescriptorHeap::new(raw_heap, self.device.clone(), descriptor_size);

        Ok(heap)
    }

    pub fn create_image(
        &mut self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        flags: D3D12_RESOURCE_FLAGS,
        state: D3D12_RESOURCE_STATES,
    ) -> Result<ImageId, DeviceError> {
        let resource_category = if format == DXGI_FORMAT_D32_FLOAT {
            ResourceCategory::RtvDsvTexture
        } else {
            ResourceCategory::OtherTexture
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: flags,
        };
        let allocation = self.allocator.create_resource(&ResourceCreateDesc {
            name: "Image",
            memory_location: MemoryLocation::GpuOnly,
            resource_category,
            resource_desc: &desc,
            clear_value: None,
            initial_state_or_layout: ResourceStateOrBarrierLayout::ResourceState(state),
            resource_type: &ResourceType::Placed,
        })?;

        let idx = self.images.len();
        self.images.push(AllocatedImage { allocation, width, height });

        Ok(ImageId(idx))
    }

    pub fn create_buffer(
        &mut self,
        size: u64,
        format: DXGI_FORMAT,
        flags: D3D12_RESOURCE_FLAGS,
        state: D3D12_RESOURCE_STATES,
        location: MemoryLocation,
    ) -> Result<BufferId, DeviceError> {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: size,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: flags,
        };
        let allocation = self.allocator.create_resource(&ResourceCreateDesc {
            name: "Buffer",
            memory_location: location,
            resource_category: ResourceCategory::Buffer,
            resource_desc: &desc,
            clear_value: None,
            initial_state_or_layout: ResourceStateOrBarrierLayout::ResourceState(state),
            resource_type: &ResourceType::Placed,
        })?;

        let idx = self.buffers.len();
        self.buffers.push(AllocatedBuffer { allocation, size });

        Ok(BufferId(idx))
    }

    pub fn create_command_encoder(
        &self,
        kind: D3D12_COMMAND_LIST_TYPE,
    ) -> Result<CommandEncoder, DeviceError> {
        let allocator: ID3D12CommandAllocator =
            unsafe { self.device.CreateCommandAllocator(kind) }?;
        let list: ID3D12GraphicsCommandList =
            unsafe { self.device.CreateCommandList(0, kind, &allocator, None) }?;
        unsafe { list.Close() }?;
        Ok(CommandEncoder::new(allocator, list))
    }

    pub fn create_root_signature(
        &self,
        flags: D3D12_ROOT_SIGNATURE_FLAGS,
        parameters: &[D3D12_ROOT_PARAMETER],
    ) -> Result<ID3D12RootSignature, DeviceError> {
        let desc = D3D12_ROOT_SIGNATURE_DESC {
            Flags: flags,
            NumParameters: parameters.len() as u32,
            pParameters: parameters.as_ptr(),
            ..Default::default()
        };
        let mut signature = None;
        unsafe {
            D3D12SerializeRootSignature(&desc, D3D_ROOT_SIGNATURE_VERSION_1, &mut signature, None)
        }?;

        let signature = signature.unwrap();
        let root_signature = unsafe {
            self.device.CreateRootSignature(
                0,
                std::slice::from_raw_parts(
                    signature.GetBufferPointer() as _,
                    signature.GetBufferSize(),
                ),
            )
        }?;
        Ok(root_signature)
    }

    pub fn create_graphics_pipeline(
        &self,
        desc: &D3D12_GRAPHICS_PIPELINE_STATE_DESC,
    ) -> Result<ID3D12PipelineState, DeviceError> {
        let pipeline = unsafe { self.device.CreateGraphicsPipelineState(desc)? };
        Ok(pipeline)
    }

    pub fn create_fence(&self) -> Result<ID3D12Fence, DeviceError> {
        let fence = unsafe { self.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }?;
        Ok(fence)
    }

    pub fn map_buffer<T>(&self, id: BufferId) -> Result<&mut [T], DeviceError> {
        let mut data = MaybeUninit::uninit();
        let buffer = &self.buffers[id.0];
        unsafe {
            buffer
                .allocation
                .resource()
                .Map(0, None, Some(data.as_mut_ptr()))?;
            let slice = std::slice::from_raw_parts_mut(data.assume_init() as *mut T, buffer.size as usize / std::mem::size_of::<T>());
            Ok(slice)
        }
    }

    pub fn unmap_buffer(&self, id: BufferId) {
        unsafe {
            self.buffers[id.0].allocation.resource().Unmap(0, None);
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        for image in self.images.drain(..) {
            let _ = self.allocator.free_resource(image.allocation);
        }

        for buffer in self.buffers.drain(..) {
            let _ = self.allocator.free_resource(buffer.allocation);
        }
    }
}

fn get_physical_device(factory: &IDXGIFactory6) -> Result<IDXGIAdapter1, Box<dyn Error>> {
    for i in 0.. {
        let physical_device: IDXGIAdapter1 =
            unsafe { factory.EnumAdapterByGpuPreference(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE) }?;

        let mut desc = Default::default();
        unsafe { physical_device.GetDesc1(&mut desc) }?;
        if (DXGI_ADAPTER_FLAG(desc.Flags as i32) & DXGI_ADAPTER_FLAG_SOFTWARE)
            != DXGI_ADAPTER_FLAG_NONE
        {
            continue;
        }

        if unsafe {
            D3D12CreateDevice(
                &physical_device,
                D3D_FEATURE_LEVEL_11_0,
                std::ptr::null_mut::<Option<ID3D12Device>>(),
            )
        }
        .is_ok()
        {
            return Ok(physical_device);
        }
    }

    unreachable!()
}

unsafe extern "system" fn message_callback(
    _category: D3D12_MESSAGE_CATEGORY,
    _severity: D3D12_MESSAGE_SEVERITY,
    _id: D3D12_MESSAGE_ID,
    description: PCSTR,
    _context: *mut c_void,
) {
    tracing::warn!("{}", description.display());
}
