pub mod asset;
pub mod camera;
mod command_encoder;
mod descriptor;
mod device;
pub mod id;
pub mod material;
pub mod mesh;
mod queue;
pub mod transform;

use asset::{Assets, Handle};
use camera::ViewUniform;
use command_encoder::CommandEncoder;
use descriptor::DescriptorHeap;
use device::Device;
use glam::{Mat4, Vec3};
use hassle_rs::{compile_hlsl, fake_sign_dxil_in_place};
use id::{BufferId, ImageId, ViewId};
use material::{GPUMaterial, Material, MaterialUniform};
use mesh::{GPUMesh, Mesh, Vertex};
use queue::Queue;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use sovereign_ecs::{CommandBuffer, PreparedQuery, World};
use std::error::Error;
use transform::{GPUTransform, GlobalTransform};
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

#[derive(Debug)]
pub struct RenderResources {
    pub vertex_buffer_id: u32,
    pub transform_buffer_id: u32,
    pub transform_offset: u32,
    pub view_buffer_index: u32,
    pub material_buffer_index: u32,
    pub material_offset: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct BufferView {
    pub buffer: BufferId,
    pub view: ViewId,
}

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

    view_buffer: BufferView,
    transform_buffer: BufferView,
    material_buffer: BufferView,
    mesh_query: PreparedQuery<(
        &'static GPUMesh,
        &'static GPUMaterial,
        &'static GPUTransform,
    )>,
    prepare_mesh_query: PreparedQuery<(&'static Handle<Mesh>,)>,
    prepare_transform_query: PreparedQuery<(&'static GlobalTransform,)>,
    prepare_material_query: PreparedQuery<(&'static Handle<Material>,)>,
}

impl Renderer {
    pub fn new(
        width: u32,
        height: u32,
        window: &dyn HasWindowHandle,
        world: &mut World,
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
        let mut cbv_heap = device.create_descriptor_heap(
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
                    Num32BitValues: (std::mem::size_of::<RenderResources>()
                        / std::mem::size_of::<u32>()) as u32,
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

        let view = ViewUniform {
            projection: Mat4::perspective_lh(
                60.0f32.to_radians(),
                width as f32 / height as f32,
                10000.0,
                0.0001,
            ),
            view: Mat4::look_at_lh(Vec3::new(-0.01, 0.005, -0.005), Vec3::ZERO, Vec3::Y),
        };
        let view_buffer = device.create_buffer(
            256,
            DXGI_FORMAT_UNKNOWN,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            MemoryLocation::CpuToGpu,
        )?;
        {
            let data = device.map_buffer::<ViewUniform>(view_buffer)?;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    &view as *const _ as *const u8,
                    data.as_ptr(),
                    std::mem::size_of::<ViewUniform>(),
                )
            };
            device.unmap_buffer(view_buffer);
        }
        let view_buffer_resource = device.get_buffer(view_buffer);
        let view_buffer_view_desc = D3D12_CONSTANT_BUFFER_VIEW_DESC {
            BufferLocation: unsafe {
                view_buffer_resource
                    .allocation
                    .resource()
                    .GetGPUVirtualAddress()
                    + view_buffer_resource
                        .allocation
                        .allocation
                        .as_ref()
                        .unwrap()
                        .offset()
            },
            SizeInBytes: device.get_buffer(view_buffer).size as u32,
        };
        let view_buffer_view = cbv_heap.create_cbv(&view_buffer_view_desc);

        let transform_buffer = device.create_buffer(
            std::mem::size_of::<GlobalTransform>() as u64 * 1000,
            DXGI_FORMAT_UNKNOWN,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            MemoryLocation::CpuToGpu,
        )?;
        let transform_buffer_view_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: 1000,
                    StructureByteStride: std::mem::size_of::<GlobalTransform>() as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let transform_buffer_view = cbv_heap.create_srv(
            device.get_buffer(transform_buffer).allocation.resource(),
            &transform_buffer_view_desc,
        );

        let material_buffer = device.create_buffer(
            std::mem::size_of::<MaterialUniform>() as u64 * 200,
            DXGI_FORMAT_UNKNOWN,
            D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON,
            MemoryLocation::CpuToGpu,
        )?;
        let material_buffer_view_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_UNKNOWN,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: 100,
                    StructureByteStride: std::mem::size_of::<MaterialUniform>() as u32,
                    Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                },
            },
        };
        let material_buffer_view = cbv_heap.create_srv(
            device.get_buffer(material_buffer).allocation.resource(),
            &material_buffer_view_desc,
        );

        world.set_singleton(Assets::<Mesh>::new());
        world.set_singleton(Assets::<Material>::new());
        let mesh_query = PreparedQuery::new();
        let prepare_mesh_query = PreparedQuery::new();
        let prepare_transform_query = PreparedQuery::new();
        let prepare_material_query = PreparedQuery::new();

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
            view_buffer: BufferView {
                buffer: view_buffer,
                view: view_buffer_view,
            },
            transform_buffer: BufferView {
                buffer: transform_buffer,
                view: transform_buffer_view,
            },
            material_buffer: BufferView {
                buffer: material_buffer,
                view: material_buffer_view,
            },
            mesh_query,
            prepare_mesh_query,
            prepare_transform_query,
            prepare_material_query,
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
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr() as *const u8,
                    data.as_ptr(),
                    std::mem::size_of::<u32>() * pixels.len(),
                )
            };
            renderer.device.unmap_buffer(buffer_id);
        }
        renderer.immediate_submit(|r, encoder| {
            let buffer = r.device.get_buffer(buffer_id);
            let error_checkboard_image = r.device.get_image(error_checkboard_image_id);
            encoder.copy_buffer_to_image(buffer, error_checkboard_image);
            encoder.transition_image(
                error_checkboard_image.allocation.resource(),
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            );
        })?;

        Ok(renderer)
    }

    pub fn prepare(&mut self, world: &mut World) -> Result<(), Box<dyn Error>> {
        let mut meshes_query = world.get_singleton::<Assets<Mesh>>();
        let (meshes,) = meshes_query.get().unwrap();

        let mut materials_query = world.get_singleton::<Assets<Material>>();
        let (materials,) = materials_query.get().unwrap();

        let mut commands = CommandBuffer::new();
        self.prepare_mesh_query
            .query(world.get())
            .iter()
            .for_each(|(entity, (mesh_handle,))| {
                let mesh = meshes.get(*mesh_handle).unwrap();
                let vertex_buffer = self
                    .device
                    .create_buffer(
                        mesh.vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64,
                        DXGI_FORMAT_UNKNOWN,
                        D3D12_RESOURCE_FLAG_NONE,
                        D3D12_RESOURCE_STATE_COMMON,
                        MemoryLocation::CpuToGpu,
                    )
                    .unwrap();
                {
                    let data = self.device.map_buffer::<Vertex>(vertex_buffer).unwrap();
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            mesh.vertices.as_ptr() as *const u8,
                            data.as_ptr(),
                            std::mem::size_of::<Vertex>() * mesh.vertices.len(),
                        )
                    };
                    self.device.unmap_buffer(vertex_buffer);
                }
                let vbv_desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_UNKNOWN,
                    ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Buffer: D3D12_BUFFER_SRV {
                            FirstElement: 0,
                            NumElements: mesh.vertices.len() as u32,
                            StructureByteStride: std::mem::size_of::<Vertex>() as u32,
                            Flags: D3D12_BUFFER_SRV_FLAG_NONE,
                        },
                    },
                };
                let vbv = self.cbv_heap.create_srv(
                    self.device.get_buffer(vertex_buffer).allocation.resource(),
                    &vbv_desc,
                );
                let index_buffer = self
                    .device
                    .create_buffer(
                        mesh.indices.len() as u64 * std::mem::size_of::<u32>() as u64,
                        DXGI_FORMAT_UNKNOWN,
                        D3D12_RESOURCE_FLAG_NONE,
                        D3D12_RESOURCE_STATE_INDEX_BUFFER,
                        MemoryLocation::CpuToGpu,
                    )
                    .unwrap();
                {
                    let data = self.device.map_buffer::<u32>(index_buffer).unwrap();
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            mesh.indices.as_ptr() as *const u8,
                            data.as_ptr(),
                            std::mem::size_of::<u32>() * mesh.indices.len(),
                        )
                    };
                    self.device.unmap_buffer(index_buffer);
                }
                commands.insert_one(
                    entity,
                    GPUMesh {
                        vertex_buffer: BufferView {
                            buffer: vertex_buffer,
                            view: vbv,
                        },
                        index_buffer,
                        index_count: mesh.indices.len(),
                    },
                );
            });

        let transform_data = self
            .device
            .map_buffer::<GlobalTransform>(self.transform_buffer.buffer)
            .unwrap();
        self.prepare_transform_query
            .query(world.get())
            .iter()
            .enumerate()
            .for_each(|(idx, (entity, (transform,)))| {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        transform as *const _ as *const u8,
                        transform_data
                            .as_ptr()
                            .offset((idx * std::mem::size_of::<GlobalTransform>()) as isize),
                        std::mem::size_of::<GlobalTransform>(),
                    )
                };
                commands.insert_one(
                    entity,
                    GPUTransform {
                        buffer: self.transform_buffer,
                        offset: idx,
                    },
                )
            });
        self.device.unmap_buffer(self.transform_buffer.buffer);

        let material_data = self
            .device
            .map_buffer::<MaterialUniform>(self.material_buffer.buffer)
            .unwrap();
        self.prepare_material_query
            .query(world.get())
            .iter()
            .enumerate()
            .for_each(|(idx, (entity, (material_idx,)))| {
                let material = materials.get(*material_idx).unwrap();
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        &material.uniform as *const _ as *const u8,
                        material_data
                            .as_ptr()
                            .offset((idx * std::mem::size_of::<MaterialUniform>()) as isize),
                        std::mem::size_of::<MaterialUniform>(),
                    )
                };
                commands.insert_one(
                    entity,
                    GPUMaterial {
                        buffer: self.material_buffer,
                        offset: idx,
                    },
                );
            });
        self.device.unmap_buffer(self.material_buffer.buffer);

        drop(meshes_query);
        drop(materials_query);

        commands.run_on(world.get_mut());
        Ok(())
    }

    pub fn render(&mut self, world: &World) -> Result<(), Box<dyn Error>> {
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
            .clear_depth_target(dsv_handle, 0.0);

        self.render_command_encoder
            .set_primitive_topology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);

        self.mesh_query.query(world.get()).iter().for_each(
            |(_entity, (mesh, material, transform))| {
                let render_resources = RenderResources {
                    vertex_buffer_id: mesh.vertex_buffer.view.0 as u32,
                    transform_buffer_id: transform.buffer.view.0 as u32,
                    transform_offset: transform.offset as u32,
                    view_buffer_index: self.view_buffer.view.0 as u32,
                    material_buffer_index: material.buffer.view.0 as u32,
                    material_offset: material.offset as u32,
                };
                self.render_command_encoder
                    .set_root_constants(&render_resources);
                self.render_command_encoder
                    .bind_index_buffer(&D3D12_INDEX_BUFFER_VIEW {
                        BufferLocation: unsafe {
                            self.device
                                .get_buffer(mesh.index_buffer)
                                .allocation
                                .resource()
                                .GetGPUVirtualAddress()
                        },
                        SizeInBytes: (mesh.index_count * std::mem::size_of::<u32>()) as u32,
                        Format: DXGI_FORMAT_R32_UINT,
                    });
                self.render_command_encoder.draw_indexed_instanced(
                    mesh.index_count as u32,
                    1,
                    0,
                    0,
                );
            },
        );

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
