use std::ffi::CString;

use ash::{Entry, Instance, ext::debug_utils, vk};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowAttributes},
};

const WINDOW_TITLE: &str = "01. Instance Creation";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

struct VulkanApp {
    window: Option<Window>,

    entry: Entry,
    instance: Instance,
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

impl VulkanApp {
    fn new(required_extensions: &[*const i8]) -> Self {
        let window = None;
        let entry = unsafe { Entry::load().expect("Failed to load Vulkan") };
        let instance = VulkanApp::create_instance(&entry, required_extensions);

        Self {
            window,
            entry,
            instance,
        }
    }

    fn create_instance(entry: &Entry, required_extensions: &[*const i8]) -> ash::Instance {
        let app_name = CString::new(WINDOW_TITLE).unwrap();
        let engine_name = CString::new("Vulkan Engine").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .engine_name(&engine_name)
            .application_version(1)
            .engine_version(1)
            .api_version(vk::make_api_version(0, 1, 4, 0));

        let mut extension_names = required_extensions.to_vec();

        extension_names.push(debug_utils::NAME.as_ptr());

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names);
        let instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Failed to create vulkan instance")
        };
        instance
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create EventLoop");

    // Continuously run the event loop. Ideal for games
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = VulkanApp::new(
        ash_window::enumerate_required_extensions(
            event_loop
                .display_handle()
                .expect("Failed to retrieve display handle")
                .as_raw(),
        )
        .unwrap(),
    );
    event_loop
        .run_app(&mut app)
        .expect("Error occured during the event loop");
}
