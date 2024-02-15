use sovereign_ecs::{Entity, EntityBuilder, ParentOf, World};
use sovereign_gltf::{load_gltf, Gltf, GltfNode};
use sovereign_render::{transform::GlobalTransform, Renderer};
use std::{error::Error, path::Path};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("Sovereign")
        .with_inner_size(PhysicalSize::new(1280, 960))
        .build(&event_loop)?;
    let mut world = World::new();
    let mut renderer = Renderer::new(1280, 960, &window, &mut world)?;
    tracing::info!("Renderer loaded");

    let gltf = load_gltf(
        &mut renderer,
        &mut world,
        &Path::new("assets/meshes/MetalRoughSpheresNoTextures.glb"),
    )?;
    for top_node in &gltf.top_nodes {
        let _ = spawn_node(&mut world, &gltf, &gltf.nodes[*top_node], None);
    }

    renderer.prepare(&mut world)?;

    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            elwt.exit();
        }
        Event::AboutToWait => {
            if let Err(err) = renderer.render(&world) {
                println!("{:?}", err);
            }
        }
        _ => {}
    })?;

    Ok(())
}

fn spawn_node(
    world: &mut World,
    gltf: &Gltf,
    node: &GltfNode,
    parent: Option<&GltfNode>,
) -> Entity {
    let mut builder = EntityBuilder::new();
    if let Some(mesh_idx) = node.mesh_idx {
        let mesh = &gltf.meshes[mesh_idx];
        let material = &gltf.materials[mesh.material_idx];
        builder.add(mesh.mesh).add(*material);
    }

    if let Some(parent) = parent {
        builder.add(GlobalTransform {
            transform: node.local_transform * parent.local_transform,
        });
    } else {
        builder.add(GlobalTransform {
            transform: node.local_transform,
        });
    }

    for children in &node.children {
        builder.add(ParentOf(spawn_node(
            world,
            gltf,
            &gltf.nodes[*children],
            Some(node),
        )));
    }
    world.spawn(builder.build())
}
