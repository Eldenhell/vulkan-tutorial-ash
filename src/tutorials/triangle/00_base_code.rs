use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

const WINDOW_TITLE: &str = "00.Base Code";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

#[derive(Debug, Default)]
struct VulkanApp {
    window: Option<Window>,
}

impl ApplicationHandler for VulkanApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window = Some(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title(WINDOW_TITLE)
                        .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
                )
                .expect("Failed to create window"),
        )
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            },
            _ => (),
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create EventLoop");

    // Continuously run the event loop. Ideal for games
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = VulkanApp::default();
    event_loop
        .run_app(&mut app)
        .expect("Error occured during the event loop");
}
