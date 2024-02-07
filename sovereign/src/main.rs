use sovereign_render::Renderer;
use std::error::Error;
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
        .with_inner_size(PhysicalSize::new(640, 480))
        .build(&event_loop)?;
    let mut renderer = Renderer::new(640, 480, &window)?;

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run(move |event, elwt| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            elwt.exit();
        }
        Event::AboutToWait => {
            if let Err(err) = renderer.render() {
                println!("{:?}", err);
            }
        }
        _ => {}
    })?;

    Ok(())
}
