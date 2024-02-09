use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use gltf::{
    scene::Transform,
    texture::{MinFilter, WrappingMode},
};
use sovereign_render::{
    id::{ImageId, SamplerId},
    material::{Material, MaterialUniform},
    mesh::Vertex,
    *,
};
use std::{error::Error, path::Path};

#[derive(Debug)]
pub struct Gltf {
    pub samplers: Vec<SamplerId>,
    pub images: Vec<ImageId>,
    pub materials: Vec<Material>,
    pub meshes: Vec<GltfMesh>,
    pub nodes: Vec<GltfNode>,
    pub top_nodes: Vec<usize>,
}

#[derive(Debug)]
pub struct GltfMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material_idx: usize,
}

#[derive(Debug)]
pub struct GltfNode {
    pub mesh_idx: Option<usize>,
    pub local_transform: Mat4,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

pub fn load_gltf(renderer: &mut Renderer, path: &Path) -> Result<Gltf, Box<dyn Error>> {
    let (document, buffers, images) = gltf::import(path)?;

    let mut samplers = Vec::new();
    let mut images = Vec::new();
    let mut materials = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut top_nodes = Vec::new();

    for sampler in document.samplers() {
        let desc = D3D12_SAMPLER_DESC {
            Filter: extract_filter(sampler.min_filter().unwrap_or(MinFilter::Nearest)),
            AddressU: extract_addressing_mode(sampler.wrap_s()),
            AddressV: extract_addressing_mode(sampler.wrap_t()),
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_BORDER,
            ..Default::default()
        };
        samplers.push(renderer.sampler_heap.create_sampler(&desc));
    }

    for _ in document.images() {
        images.push(renderer.checkerboard_image);
    }

    let material_buffer = renderer.device.create_buffer(
        (document.materials().len() * std::mem::size_of::<MaterialUniform>()) as u64,
        DXGI_FORMAT_UNKNOWN,
        D3D12_RESOURCE_FLAG_NONE,
        D3D12_RESOURCE_STATE_COMMON,
        MemoryLocation::CpuToGpu,
    )?;
    let material_data = renderer
        .device
        .map_buffer::<MaterialUniform>(material_buffer)?;
    let mut idx = 0;
    for material in document.materials() {
        let uniform = MaterialUniform {
            base_color_factors: Vec4::from_array(
                material.pbr_metallic_roughness().base_color_factor(),
            ),
            metal_rough_factors: Vec2::new(
                material.pbr_metallic_roughness().metallic_factor(),
                material.pbr_metallic_roughness().roughness_factor(),
            ),
        };
        material_data[idx] = uniform.clone();

        let (color_image, color_sampler) = if let Some(base_color_texture) =
            material.pbr_metallic_roughness().base_color_texture()
        {
            let image_idx = base_color_texture.texture().source().index();
            let sampler_idx = base_color_texture.texture().sampler().index().unwrap();
            (Some(images[image_idx]), Some(samplers[sampler_idx]))
        } else {
            (None, None)
        };

        materials.push(Material {
            uniform,
            buffer: material_buffer,
            offset: idx,
            color_image,
            color_sampler,
        });

        idx += 1;
    }
    renderer.device.unmap_buffer(material_buffer);

    for gltf_mesh in document.meshes() {
        for primitive in gltf_mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions = reader
                .read_positions()
                .unwrap()
                .map(|p| Vec3::from_array(p))
                .collect::<Vec<_>>();
            let normals = reader
                .read_normals()
                .unwrap()
                .map(|n| Vec3::from_array(n))
                .collect::<Vec<_>>();
            let colors = reader.read_colors(0).map(|c| {
                c.into_rgba_f32()
                    .map(|c| Vec4::from_array(c))
                    .collect::<Vec<_>>()
            });
            let uvs = reader
                .read_tex_coords(0)
                .unwrap()
                .into_f32()
                .map(|u| Vec2::from_array(u))
                .collect::<Vec<_>>();

            let indices = reader
                .read_indices()
                .unwrap()
                .into_u32()
                .collect::<Vec<_>>();
            let mut vertices = Vec::new();
            for i in 0..positions.len() {
                vertices.push(Vertex {
                    position: positions[i],
                    normal: normals[i],
                    color: colors.as_ref().map(|c| c[i]).unwrap_or_else(|| Vec4::ONE),
                    uv: uvs[i],
                });
            }

            meshes.push(GltfMesh {
                vertices,
                indices,
                material_idx: primitive.material().index().unwrap(),
            });
        }
    }

    for gltf_node in document.nodes() {
        let mesh_idx = gltf_node.mesh().map(|m| m.index());
        let transform = match gltf_node.transform() {
            Transform::Matrix { matrix } => Mat4::from_cols_array_2d(&matrix),
            Transform::Decomposed {
                translation,
                rotation,
                scale,
            } => {
                let translation = Vec3::from_array(translation);
                let rotation = Quat::from_array(rotation);
                let scale = Vec3::from_array(scale);
                Mat4::from_scale_rotation_translation(scale, rotation, translation)
            }
        };
        nodes.push(GltfNode {
            mesh_idx,
            local_transform: transform,
            parent: None,
            children: Vec::new(),
        });
    }

    for (i, gltf_node) in document.nodes().enumerate() {
        for child in gltf_node.children() {
            nodes[i].children.push(child.index());
            nodes[child.index()].parent = Some(i);
        }
    }

    for (idx, node) in nodes.iter().enumerate() {
        if node.parent.is_none() {
            top_nodes.push(idx);
        }
    }

    Ok(Gltf {
        samplers,
        images,
        materials,
        meshes,
        nodes,
        top_nodes,
    })
}

fn extract_filter(filter: MinFilter) -> D3D12_FILTER {
    match filter {
        MinFilter::Nearest => D3D12_FILTER_MIN_MAG_MIP_POINT,
        MinFilter::Linear => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
        MinFilter::NearestMipmapNearest => D3D12_FILTER_MIN_MAG_MIP_POINT,
        MinFilter::LinearMipmapNearest => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
        MinFilter::NearestMipmapLinear => D3D12_FILTER_MIN_MAG_MIP_POINT,
        MinFilter::LinearMipmapLinear => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
    }
}

fn extract_addressing_mode(mode: WrappingMode) -> D3D12_TEXTURE_ADDRESS_MODE {
    match mode {
        WrappingMode::ClampToEdge => D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
        WrappingMode::MirroredRepeat => D3D12_TEXTURE_ADDRESS_MODE_MIRROR,
        WrappingMode::Repeat => D3D12_TEXTURE_ADDRESS_MODE_WRAP,
    }
}
