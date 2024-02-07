mod command_encoder;
mod descriptor;
mod device;
mod queue;

use command_encoder::CommandEncoder;
use descriptor::DescriptorHeap;
use device::{AllocatedImage, Device};
use queue::Queue;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::error::Error;
use windows::Win32::{
    Foundation::{HANDLE, HWND},
    Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D12::*,
        Dxgi::{Common::*, *},
    },
    System::Threading::{CreateEventA, WaitForSingleObject},
};

pub struct Renderer {
    width: u32,
    height: u32,

    device: Device,
    graphics_queue: Queue,
    swapchain: IDXGISwapChain3,
    rtv_heap: DescriptorHeap,
    dsv_heap: DescriptorHeap,
    cbv_heap: DescriptorHeap,
    sampler_heap: DescriptorHeap,
    command_encoder: CommandEncoder,
    root_signature: ID3D12RootSignature,
    //pipeline: ID3D12PipelineState,
    render_targets: Vec<ID3D12Resource>,
    depth_texture: AllocatedImage,
    frame_index: usize,

    fence: ID3D12Fence,
    fence_value: u64,
    fence_event: HANDLE,
}

impl Renderer {
    pub fn new(
        width: u32,
        height: u32,
        window: &dyn HasWindowHandle,
    ) -> Result<Self, Box<dyn Error>> {
        let mut device = Device::new()?;
        let graphics_queue = device.create_command_queue(D3D12_COMMAND_LIST_TYPE_DIRECT)?;

        let hwnd = match window.window_handle()?.as_raw() {
            RawWindowHandle::Win32(win) => HWND(win.hwnd.get()),
            _ => unreachable!(),
        };
        let swapchain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };
        let swapchain = device.create_swapchain(&swapchain_desc, &graphics_queue, hwnd)?;

        let mut rtv_heap = device.create_descriptor_heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            2,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        let mut dsv_heap = device.create_descriptor_heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        let cbv_heap = device.create_descriptor_heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            1000,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        )?;
        let sampler_heap = device.create_descriptor_heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
            1000,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        )?;

        let mut render_targets = Vec::new();
        for i in 0..2 {
            let render_target: ID3D12Resource = unsafe { swapchain.GetBuffer(i)? };
            rtv_heap.create_rtv(&render_target);
            render_targets.push(render_target);
        }

        let depth_texture = device.create_image(
            width,
            height,
            DXGI_FORMAT_D32_FLOAT,
            D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            D3D12_RESOURCE_STATE_DEPTH_WRITE,
        )?;
        dsv_heap.create_dsv(depth_texture.allocation.resource());

        let command_encoder = device.create_command_encoder(D3D12_COMMAND_LIST_TYPE_DIRECT)?;

        let root_signature = device
            .create_root_signature(D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED)?;
        let mut pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: unsafe { std::mem::transmute_copy(&root_signature) },
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                ..Default::default()
            },
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: [
                    D3D12_RENDER_TARGET_BLEND_DESC {
                        BlendEnable: false.into(),
                        LogicOpEnable: false.into(),
                        SrcBlend: D3D12_BLEND_ONE,
                        DestBlend: D3D12_BLEND_ZERO,
                        BlendOp: D3D12_BLEND_OP_ADD,
                        SrcBlendAlpha: D3D12_BLEND_ONE,
                        DestBlendAlpha: D3D12_BLEND_ZERO,
                        BlendOpAlpha: D3D12_BLEND_OP_ADD,
                        LogicOp: D3D12_LOGIC_OP_NOOP,
                        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
                    },
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                    D3D12_RENDER_TARGET_BLEND_DESC::default(),
                ],
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ALL,
                DepthFunc: D3D12_COMPARISON_FUNC_GREATER,
                ..Default::default()
            },
            SampleMask: u32::MAX,
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        pipeline_desc.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
        //let pipeline = device.create_graphics_pipeline(&pipeline_desc)?;

        let fence = device.create_fence()?;
        let fence_value = 1;
        let fence_event = unsafe { CreateEventA(None, false, false, None) }?;

        let frame_index = unsafe { swapchain.GetCurrentBackBufferIndex() } as usize;

        Ok(Self {
            width,
            height,
            device,
            graphics_queue,
            swapchain,
            rtv_heap,
            dsv_heap,
            cbv_heap,
            sampler_heap,
            render_targets,
            depth_texture,
            frame_index,
            command_encoder,
            root_signature,
            //pipeline,
            fence,
            fence_event,
            fence_value,
        })
    }

    pub fn render(&mut self) -> Result<(), Box<dyn Error>> {
        self.command_encoder.reset()?;

        self.command_encoder
            .set_root_signature(&self.root_signature);
        self.command_encoder.set_viewport(self.width, self.height);
        self.command_encoder.set_scissor(self.width, self.height);

        self.command_encoder.transition_image(
            &self.render_targets[self.frame_index],
            D3D12_RESOURCE_STATE_PRESENT,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
        );

        let rtv_handle = self.rtv_heap.get_handle(self.frame_index);
        let dsv_handle = self.dsv_heap.get_handle(0);
        self.command_encoder
            .set_render_target(rtv_handle, Some(&dsv_handle));

        self.command_encoder
            .clear_render_target(rtv_handle, &[0.0, 0.0, 0.0, 1.0]);
        self.command_encoder.clear_depth_target(dsv_handle, 1.0);

        self.command_encoder
            .set_primitive_topology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

        self.command_encoder.transition_image(
            &self.render_targets[self.frame_index],
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT,
        );

        let command_list = self.command_encoder.finish()?;
        self.graphics_queue
            .execute_command_lists(&[Some(command_list)]);

        unsafe { self.swapchain.Present(1, 0) }.ok()?;

        self.wait_for_previous_frame()?;

        Ok(())
    }

    pub fn wait_for_previous_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let fence_value = self.fence_value;
        self.graphics_queue.signal(&self.fence, fence_value)?;
        self.fence_value += 1;

        if unsafe { self.fence.GetCompletedValue() } < fence_value {
            unsafe {
                self.fence
                    .SetEventOnCompletion(fence_value, self.fence_event)?;
                WaitForSingleObject(self.fence_event, u32::MAX);
            }
        }

        self.frame_index = unsafe { self.swapchain.GetCurrentBackBufferIndex() } as usize;
        Ok(())
    }
}
