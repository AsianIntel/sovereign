mod command_encoder;
mod descriptor;
mod device;
pub mod id;
pub mod material;
pub mod mesh;
mod queue;

use command_encoder::CommandEncoder;
use descriptor::DescriptorHeap;
use device::Device;
use hassle_rs::{compile_hlsl, fake_sign_dxil_in_place};
use id::ImageId;
use queue::Queue;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::error::Error;
use windows::Win32::{
    Foundation::{HANDLE, HWND},
    System::Threading::{CreateEventA, WaitForSingleObject},
};

pub use gpu_allocator::MemoryLocation;
pub use windows::Win32::Graphics::{
    Direct3D::*,
    Direct3D12::*,
    Dxgi::{Common::*, *},
};

pub struct Renderer {
    width: u32,
    height: u32,

    pub device: Device,
    graphics_queue: Queue,
    swapchain: IDXGISwapChain3,
    rtv_heap: DescriptorHeap,
    dsv_heap: DescriptorHeap,
    pub cbv_heap: DescriptorHeap,
    pub sampler_heap: DescriptorHeap,
    root_signature: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    render_targets: Vec<ID3D12Resource>,
    frame_index: usize,

    render_command_encoder: CommandEncoder,
    immediate_command_encoder: CommandEncoder,

    fence: ID3D12Fence,
    fence_value: u64,
    fence_event: HANDLE,

    pub checkerboard_image: ImageId,
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
        dsv_heap.create_dsv(device.get_image(depth_texture));

        let render_command_encoder =
            device.create_command_encoder(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
        let immediate_command_encoder =
            device.create_command_encoder(D3D12_COMMAND_LIST_TYPE_DIRECT)?;

        let shader_code = std::fs::read_to_string("assets/shaders/mesh.hlsl")?;
        let mut vertex_shader =
            compile_hlsl("mesh.hlsl", &shader_code, "VSMain", "vs_6_6", &[], &[])?;
        let mut fragment_shader =
            compile_hlsl("mesh.hlsl", &shader_code, "PSMain", "ps_6_6", &[], &[])?;
        fake_sign_dxil_in_place(&mut vertex_shader);
        fake_sign_dxil_in_place(&mut fragment_shader);

        let constants = D3D12_ROOT_PARAMETER {
            ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            Anonymous: D3D12_ROOT_PARAMETER_0 {
                Constants: D3D12_ROOT_CONSTANTS {
                    ShaderRegister: 0,
                    RegisterSpace: 0,
                    Num32BitValues: 5,
                },
            },
        };
        let root_signature = device.create_root_signature(
            D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED,
            &[constants],
        )?;
        let mut pipeline_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: unsafe { std::mem::transmute_copy(&root_signature) },
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vertex_shader.as_ptr() as *const _,
                BytecodeLength: vertex_shader.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: fragment_shader.as_ptr() as *const _,
                BytecodeLength: fragment_shader.len(),
            },
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
            DSVFormat: DXGI_FORMAT_D32_FLOAT,
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
        let pipeline = device.create_graphics_pipeline(&pipeline_desc)?;

        let fence = device.create_fence()?;
        let fence_value = 1;
        let fence_event = unsafe { CreateEventA(None, false, false, None) }?;

        let frame_index = unsafe { swapchain.GetCurrentBackBufferIndex() } as usize;

        let mut renderer = Self {
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
            frame_index,
            render_command_encoder,
            immediate_command_encoder,
            root_signature,
            pipeline,
            fence,
            fence_event,
            fence_value,
            checkerboard_image: ImageId(0),
        };

        let magenta = 0xFFFF00FFu32;
        let black = 0xFF000000;
        let mut pixels = vec![0; 16 * 16];
        for i in 0..16 {
            for j in 0..16 {
                pixels[j * 16 + i] = if ((i % 2) ^ (j % 2)) != 0 {
                    magenta
                } else {
                    black
                };
            }
        }
        let error_checkboard_image_id = renderer.device.create_image(
            16,
            16,
            DXGI_FORMAT_R8G8B8A8_UNORM,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        renderer.checkerboard_image = error_checkboard_image_id;

        let buffer_id = renderer
            .device
            .create_buffer(
                16 * 16 * 4,
                DXGI_FORMAT_UNKNOWN,
                D3D12_RESOURCE_FLAG_NONE,
                D3D12_RESOURCE_STATE_COMMON,
                MemoryLocation::CpuToGpu,
            )
            .unwrap();
        {
            let data = renderer.device.map_buffer::<u8>(buffer_id)?;
            data.copy_from_slice(bytemuck::cast_slice(&pixels));
            renderer.device.unmap_buffer(buffer_id);
        }
        renderer.immediate_submit(|r, encoder| {
            let buffer = r.device.get_buffer(buffer_id);
            let error_checkboard_image = r.device.get_image(error_checkboard_image_id);
            encoder.copy_buffer_to_image(buffer, error_checkboard_image);
        })?;

        Ok(renderer)
    }

    pub fn render(&mut self) -> Result<(), Box<dyn Error>> {
        self.render_command_encoder.reset()?;

        self.render_command_encoder
            .set_descriptor_heaps(&[Some(self.cbv_heap.get()), Some(self.sampler_heap.get())]);
        self.render_command_encoder
            .set_root_signature(&self.root_signature);
        self.render_command_encoder.set_pipeline(&self.pipeline);
        self.render_command_encoder
            .set_viewport(self.width, self.height);
        self.render_command_encoder
            .set_scissor(self.width, self.height);

        self.render_command_encoder.transition_image(
            &self.render_targets[self.frame_index],
            D3D12_RESOURCE_STATE_PRESENT,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
        );

        let rtv_handle = self.rtv_heap.get_handle(self.frame_index);
        let dsv_handle = self.dsv_heap.get_handle(0);
        self.render_command_encoder
            .set_render_target(rtv_handle, Some(&dsv_handle));

        self.render_command_encoder
            .clear_render_target(rtv_handle, &[0.0, 0.0, 0.0, 1.0]);
        self.render_command_encoder
            .clear_depth_target(dsv_handle, 1.0);

        self.render_command_encoder
            .set_primitive_topology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

        self.render_command_encoder.transition_image(
            &self.render_targets[self.frame_index],
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT,
        );

        let command_list = self.render_command_encoder.finish()?;
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

    pub fn immediate_submit(
        &mut self,
        f: impl Fn(&Renderer, &CommandEncoder),
    ) -> Result<(), Box<dyn Error>> {
        self.immediate_command_encoder.reset()?;

        f(self, &self.immediate_command_encoder);

        let command_list = self.immediate_command_encoder.finish()?;
        self.graphics_queue
            .execute_command_lists(&[Some(command_list)]);

        self.wait_for_previous_frame()?;
        Ok(())
    }
}
