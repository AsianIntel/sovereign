use glam::{Mat4, Vec3, Vec4};
use sovereign_ecs::{Entity, EntityBuilder, ParentOf, PreparedQuery, World};
use sovereign_gltf::{load_gltf, Gltf, GltfNode};
use sovereign_render::{camera::Camera, transform::GlobalTransform, Renderer};
use std::{error::Error, path::Path};
use winit::{
    dpi::PhysicalSize, event::{DeviceEvent, Event, WindowEvent}, event_loop::{ControlFlow, EventLoop}, keyboard::{KeyCode, PhysicalKey}, window::WindowBuilder
};

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let width = 1280;
    let height = 960;

    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("Sovereign")
        .with_inner_size(PhysicalSize::new(width, height))
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

    world.spawn((
        Camera {
            projection: Mat4::perspective_lh(
                60.0f32.to_radians(),
                width as f32 / height as f32,
                10000.0,
                0.0001,
            ),
        },
        GlobalTransform {
            transform: Mat4::look_at_lh(Vec3::new(-0.01, 0.005, -0.005), Vec3::ZERO, Vec3::Y)
                .inverse(),
        },
    ));

    renderer.prepare(&mut world)?;

    let mut camera_query: PreparedQuery<(&'static Camera, &'static mut GlobalTransform)> = PreparedQuery::new();

    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            elwt.exit();
        }
        Event::AboutToWait => {
            if let Err(err) = renderer.prepare_render(&world) {
                println!("{:?}", err);
            }
            if let Err(err) = renderer.render(&world) {
                println!("{:?}", err);
            }
        },
        Event::DeviceEvent { event: DeviceEvent::Key(raw_key_event), .. } => {
            if let PhysicalKey::Code(key) = raw_key_event.physical_key {
                if key == KeyCode::KeyW {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let forward = transform.transform * Vec4::Z;
                        let forward = forward.normalize() * 0.0001;
                        transform.transform.w_axis += forward;
                    });
                } else if key == KeyCode::KeyS {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let forward = transform.transform * -Vec4::Z;
                        let forward = forward.normalize() * 0.0001;
                        transform.transform.w_axis += forward;
                    });
                }
                if key == KeyCode::KeyD {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let right = transform.transform * Vec4::X;
                        let right = right.normalize() * 0.0001;
                        transform.transform.w_axis += right;
                    });
                } else if key == KeyCode::KeyA {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let right = transform.transform * -Vec4::X;
                        let right = right.normalize() * 0.0001;
                        transform.transform.w_axis += right;
                    });
                }
                if key == KeyCode::KeyE {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let up = transform.transform * Vec4::Y;
                        let up = up.normalize() * 0.0001;
                        transform.transform.w_axis += up;
                    });
                } else if key == KeyCode::KeyQ {
                    camera_query.query(world.get()).iter().for_each(|(_entity, (_camera, transform))| {
                        let up = transform.transform * -Vec4::Y;
                        let up = up.normalize() * 0.0001;
                        transform.transform.w_axis += up;
                    });
                }
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
