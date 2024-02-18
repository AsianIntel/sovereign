use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use gltf::{
    scene::Transform,
    texture::{MinFilter, WrappingMode},
};
use sovereign_ecs::World;
use sovereign_render::{
    asset::{Assets, Handle},
    id::{ImageId, SamplerId},
    material::{Material, MaterialUniform},
    mesh::{Mesh, Vertex},
    *,
};
use std::{error::Error, path::Path};

#[derive(Debug)]
pub struct Gltf {
    pub samplers: Vec<SamplerId>,
    pub images: Vec<ImageId>,
    pub materials: Vec<Handle<Material>>,
    pub meshes: Vec<GltfMesh>,
    pub nodes: Vec<GltfNode>,
    pub top_nodes: Vec<usize>,
}

#[derive(Debug)]
pub struct GltfMesh {
    pub mesh: Handle<Mesh>,
    pub material_idx: usize,
}

#[derive(Debug)]
pub struct GltfNode {
    pub mesh_idx: Option<usize>,
    pub local_transform: Mat4,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

pub fn load_gltf(
    renderer: &mut Renderer,
    world: &mut World,
    path: &Path,
) -> Result<Gltf, Box<dyn Error>> {
    let (document, buffers, _images) = gltf::import(path)?;

    let mut meshes_query = world.get_singleton::<Assets<Mesh>>();
    let (asset_meshes,) = meshes_query.get().unwrap();

    let mut materials_query = world.get_singleton::<Assets<Material>>();
    let (asset_materials,) = materials_query.get().unwrap();

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

    for material in document.materials() {
        let uniform = MaterialUniform {
            base_color_factors: Vec4::from_array(
                material.pbr_metallic_roughness().base_color_factor(),
            ),
            perceptual_roughness: material.pbr_metallic_roughness().roughness_factor(),
            metallic: material.pbr_metallic_roughness().metallic_factor(),
            reflectance: 0.5,
            pad: 0.0,
        };

        let (color_image, color_sampler) = if let Some(base_color_texture) =
            material.pbr_metallic_roughness().base_color_texture()
        {
            let image_idx = base_color_texture.texture().source().index();
            let sampler_idx = base_color_texture.texture().sampler().index().unwrap();
            (Some(images[image_idx]), Some(samplers[sampler_idx]))
        } else {
            (None, None)
        };

        materials.push(asset_materials.push(Material {
            uniform,
            color_image,
            color_sampler,
        }));
    }

    for gltf_mesh in document.meshes() {
        for primitive in gltf_mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions = reader
                .read_positions()
                .unwrap()
                .map(|p| Vec4::new(p[0], p[1], p[2], 1.0))
                .collect::<Vec<_>>();
            let normals = reader
                .read_normals()
                .unwrap()
                .map(|n| Vec4::new(n[0], n[1], n[2], 1.0))
                .collect::<Vec<_>>();
            let colors = reader.read_colors(0).map(|c| {
                c.into_rgba_f32()
                    .map(|c| Vec4::from_array(c))
                    .collect::<Vec<_>>()
            });
            let uvs = reader.read_tex_coords(0).map(|u| {
                u.into_f32()
                    .map(|u| Vec2::from_array(u))
                    .collect::<Vec<_>>()
            });

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
                    uv: uvs.as_ref().map(|u| u[i]).unwrap_or_else(|| Vec2::ZERO),
                    pad: Vec2::ZERO,
                });
            }

            meshes.push(GltfMesh {
                mesh: asset_meshes.push(Mesh { vertices, indices }),
                material_idx: primitive.material().index().unwrap_or(0),
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
