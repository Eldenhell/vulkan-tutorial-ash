use std::{
    borrow::Cow,
    ffi::{self, CStr, CString},
    io::Cursor,
    mem::{offset_of, size_of},
    os::raw::{c_char, c_void},
    time::Instant,
};

use ash::{
    Entry, Instance,
    ext::debug_utils,
    khr::{surface, swapchain},
    util::{Align, read_spv},
    vk,
};
use glam::{Mat4, Vec3};
use vulkan_tuto_ash::utils::{self, vk_to_string};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    raw_window_handle::{DisplayHandle, HasDisplayHandle, HasWindowHandle, RawDisplayHandle},
    window::{Window, WindowAttributes},
};

const WINDOW_TITLE: &str = "23. Combined image sampler";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

const MAX_FRAMES_IN_FLIGHT: u32 = 2;

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = unsafe { *p_callback_data };
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        unsafe { ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy() }
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        unsafe { ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy() }
    };

    println!(
        "{message_severity:?}:\n{message_type:?} [{message_id_name} ({message_id_number})] : {message}\n",
    );

    vk::FALSE
}

const VERTICES: [Vertex; 4] = [
    Vertex {
        pos: [-0.5, -0.5],
        color: [1.0, 0.0, 0.0],
        tex_coord: [1.0, 0.0],
    },
    Vertex {
        pos: [0.5, -0.5],
        color: [0.0, 1.0, 0.0],
        tex_coord: [0.0, 0.0],
    },
    Vertex {
        pos: [0.5, 0.5],
        color: [0.0, 0.0, 1.0],
        tex_coord: [0.0, 1.0],
    },
    Vertex {
        pos: [-0.5, 0.5],
        color: [1.0, 1.0, 1.0],
        tex_coord: [1.0, 1.0],
    },
];

// We can use u32 if u16 isn't enough
const INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

#[derive(Clone, Debug, Copy)]
struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 3],
    pub tex_coord: [f32; 2], //uv
}

impl Vertex {
    fn get_binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<Vertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 3] {
        [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, pos) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: offset_of!(Vertex, color) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, tex_coord) as u32,
            },
        ]
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct UniformBufferObject {
    model: Mat4,
    view: Mat4,
    proj: Mat4,
}

#[allow(unused)]
struct SwapchainData {
    pub loader: swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub surface_format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
}

struct VulkanApp {
    window: Option<Window>,
    surface: Option<vk::SurfaceKHR>,

    entry: Entry,
    instance: Instance,
    surface_loader: surface::Instance,
    debug_utils_loader: debug_utils::Instance,
    debug_callback: vk::DebugUtilsMessengerEXT,

    physical_device: vk::PhysicalDevice,
    device: Option<ash::Device>,
    graphics_queue_index: Option<u32>,
    graphics_queue: Option<vk::Queue>,
    swapchain_data: Option<SwapchainData>,
    swapchain_image_views: Option<Vec<vk::ImageView>>,

    graphics_pipeline: Option<vk::Pipeline>,
    descriptor_set_layout: Option<vk::DescriptorSetLayout>,
    pipeline_layout: Option<vk::PipelineLayout>,

    command_pool: Option<vk::CommandPool>,
    vertex_staging_buffer: Option<vk::Buffer>,
    vertex_staging_buffer_memory: Option<vk::DeviceMemory>,
    vertex_buffer: Option<vk::Buffer>,
    vertex_buffer_memory: Option<vk::DeviceMemory>,
    command_buffers: Option<Vec<vk::CommandBuffer>>,

    index_staging_buffer: Option<vk::Buffer>,
    index_staging_buffer_memory: Option<vk::DeviceMemory>,
    index_buffer: Option<vk::Buffer>,
    index_buffer_memory: Option<vk::DeviceMemory>,

    uniform_buffers: Option<[vk::Buffer; MAX_FRAMES_IN_FLIGHT as usize]>,
    uniform_buffer_memories: Option<[vk::DeviceMemory; MAX_FRAMES_IN_FLIGHT as usize]>,
    uniform_buffers_mapped: Option<[*mut c_void; MAX_FRAMES_IN_FLIGHT as usize]>,

    descriptor_pool: Option<vk::DescriptorPool>,
    descriptor_sets: Option<Vec<vk::DescriptorSet>>,

    texture_image: Option<vk::Image>,
    texture_image_memory: Option<vk::DeviceMemory>,
    texture_image_view: Option<vk::ImageView>,
    texture_image_sampler: Option<vk::Sampler>,

    present_complete_semaphores: Option<Vec<vk::Semaphore>>,
    render_finished_semaphores: Option<Vec<vk::Semaphore>>,
    in_flight_fences: Option<Vec<vk::Fence>>,

    frame_index: usize,
    #[allow(unused)]
    fps_counter: utils::FpsCounter,
    start_time: Instant,
}

impl ApplicationHandler for VulkanApp {
    // We need to create / recreate everything related to window aka surface, swapchain, ..
    // For cross-platform compatibility, especially on Android
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title(WINDOW_TITLE)
                    .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
            )
            .expect("Failed to create window");

        let display_handle = event_loop
            .display_handle()
            .expect("Failed to retrieve display handle")
            .as_raw();

        let surface =
            VulkanApp::create_surface(&self.entry, &self.instance, &display_handle, &window);

        let (device, graphics_queue_index) = VulkanApp::create_logical_device(
            &self.instance,
            &self.physical_device,
            &self.surface_loader,
            &surface,
        );

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_index, 0) };

        let swapchain_data = VulkanApp::create_swapchain(
            &self.instance,
            &self.physical_device,
            &device,
            &self.surface_loader,
            &surface,
        );

        let swapchain_image_views = VulkanApp::create_image_views(&device, &swapchain_data);

        let descriptor_set_layout = VulkanApp::create_descriptor_set_layout(&device);

        let (graphics_pipeline, pipeline_layout) =
            VulkanApp::create_graphics_pipeline(&device, &swapchain_data, &descriptor_set_layout);

        let command_pool = VulkanApp::create_command_pool(&device, graphics_queue_index);

        let (texture_image, texture_image_memory) = VulkanApp::create_texture_image(
            &self.instance,
            &device,
            &self.physical_device,
            &graphics_queue,
            &command_pool,
        );

        let texture_image_view = VulkanApp::create_texture_image_view(&device, &texture_image);

        let texture_image_sampler =
            VulkanApp::create_texture_image_sampler(&self.instance, &device, &self.physical_device);

        let vertex_buffers = VulkanApp::create_vertex_buffer(
            &self.instance,
            &device,
            &self.physical_device,
            &command_pool,
            &graphics_queue,
        );

        let (vertex_staging_buffer, vertex_staging_buffer_memory) = vertex_buffers[0];
        let (vertex_buffer, vertex_buffer_memory) = vertex_buffers[1];

        let index_buffers = VulkanApp::create_index_buffer(
            &self.instance,
            &device,
            &self.physical_device,
            &command_pool,
            &graphics_queue,
        );

        let (index_staging_buffer, index_staging_buffer_memory) = index_buffers[0];
        let (index_buffer, index_buffer_memory) = index_buffers[1];

        let (uniform_buffers, uniform_buffer_memories, uniform_buffers_mapped) =
            VulkanApp::create_uniform_buffers(&self.instance, &device, &self.physical_device);

        let descriptor_pool = VulkanApp::create_descriptor_pool(&device);

        let descriptor_sets = VulkanApp::create_descriptor_sets(
            &device,
            &descriptor_pool,
            &descriptor_set_layout,
            &uniform_buffers,
            &texture_image_sampler,
            &texture_image_view,
        );

        let command_buffers = VulkanApp::create_command_buffers(&device, &command_pool);

        let (present_complete_semaphores, render_finished_semaphores, in_flight_fences) =
            VulkanApp::create_sync_objs(&device, &swapchain_data);

        self.surface = Some(surface);
        self.window = Some(window);
        self.device = Some(device);
        self.graphics_queue_index = Some(graphics_queue_index);
        self.graphics_queue = Some(graphics_queue);
        self.swapchain_data = Some(swapchain_data);
        self.swapchain_image_views = Some(swapchain_image_views);
        self.graphics_pipeline = Some(graphics_pipeline);
        self.descriptor_set_layout = Some(descriptor_set_layout);
        self.pipeline_layout = Some(pipeline_layout);
        self.command_pool = Some(command_pool);
        self.texture_image = Some(texture_image);
        self.texture_image_memory = Some(texture_image_memory);
        self.texture_image_view = Some(texture_image_view);
        self.texture_image_sampler = Some(texture_image_sampler);
        self.vertex_staging_buffer = Some(vertex_staging_buffer);
        self.vertex_staging_buffer_memory = Some(vertex_staging_buffer_memory);
        self.vertex_buffer = Some(vertex_buffer);
        self.vertex_buffer_memory = Some(vertex_buffer_memory);
        self.index_staging_buffer = Some(index_staging_buffer);
        self.index_staging_buffer_memory = Some(index_staging_buffer_memory);
        self.index_buffer = Some(index_buffer);
        self.index_buffer_memory = Some(index_buffer_memory);
        self.uniform_buffers = Some(uniform_buffers);
        self.uniform_buffer_memories = Some(uniform_buffer_memories);
        self.uniform_buffers_mapped = Some(uniform_buffers_mapped);
        self.command_buffers = Some(command_buffers);
        self.descriptor_pool = Some(descriptor_pool);
        self.descriptor_sets = Some(descriptor_sets);
        self.present_complete_semaphores = Some(present_complete_semaphores);
        self.render_finished_semaphores = Some(render_finished_semaphores);
        self.in_flight_fences = Some(in_flight_fences);
        self.start_time = Instant::now();
    }

    // Here we need to drop window, surface, swapchains, etc
    #[allow(unused)]
    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        unsafe {
            if let Some(device) = self.device.take() {
                #[allow(clippy::unwrap_used)]
                device.device_wait_idle().unwrap();

                if let Some(present_complete_semaphores) = self.present_complete_semaphores.take() {
                    for sem in present_complete_semaphores {
                        device.destroy_semaphore(sem, None);
                    }
                }

                if let Some(render_finished_semaphores) = self.render_finished_semaphores.take() {
                    for sem in render_finished_semaphores {
                        device.destroy_semaphore(sem, None);
                    }
                }

                if let Some(in_flight_fence) = self.in_flight_fences.take() {
                    for fence in in_flight_fence {
                        device.destroy_fence(fence, None);
                    }
                }

                if let Some(sampler) = self.texture_image_sampler.take() {
                    device.destroy_sampler(sampler, None);
                }

                if let Some(im_view) = self.texture_image_view.take() {
                    device.destroy_image_view(im_view, None);
                }

                if let Some(mem) = self.texture_image_memory.take() {
                    device.free_memory(mem, None);
                }

                if let Some(im) = self.texture_image.take() {
                    device.destroy_image(im, None);
                }

                if let Some(vertex_buffer_memory) = self.vertex_buffer_memory.take() {
                    device.free_memory(vertex_buffer_memory, None);
                }

                if let Some(vertex_staging_buffer_memory) = self.vertex_staging_buffer_memory.take()
                {
                    device.free_memory(vertex_staging_buffer_memory, None);
                }

                if let Some(vertex_buffer) = self.vertex_buffer.take() {
                    device.destroy_buffer(vertex_buffer, None);
                }

                if let Some(vertex_staging_buffer) = self.vertex_staging_buffer.take() {
                    device.destroy_buffer(vertex_staging_buffer, None);
                }

                if let Some(index_buffer_memory) = self.index_buffer_memory.take() {
                    device.free_memory(index_buffer_memory, None);
                }

                if let Some(index_staging_buffer_memory) = self.index_staging_buffer_memory.take() {
                    device.free_memory(index_staging_buffer_memory, None);
                }

                if let Some(uniform_buffer_memories) = self.uniform_buffer_memories.take() {
                    for mem in uniform_buffer_memories {
                        device.unmap_memory(mem);
                        device.free_memory(mem, None);
                    }
                }

                if let Some(index_buffer) = self.index_buffer.take() {
                    device.destroy_buffer(index_buffer, None);
                }

                if let Some(index_staging_buffer) = self.index_staging_buffer.take() {
                    device.destroy_buffer(index_staging_buffer, None);
                }

                if let Some(uniform_buffers) = self.uniform_buffers.take() {
                    for buf in uniform_buffers {
                        device.destroy_buffer(buf, None);
                    }
                }

                if let Some(layout) = self.descriptor_set_layout.take() {
                    device.destroy_descriptor_set_layout(layout, None);
                }

                if let Some(layout) = self.pipeline_layout.take() {
                    device.destroy_pipeline_layout(layout, None);
                }

                if let Some(pipeline) = self.graphics_pipeline.take() {
                    device.destroy_pipeline(pipeline, None);
                }

                if let Some(descriptor_pool) = self.descriptor_pool.take() {
                    device.destroy_descriptor_pool(descriptor_pool, None);
                }

                if let Some(image_views) = self.swapchain_image_views.take() {
                    for image_view in image_views {
                        device.destroy_image_view(image_view, None);
                    }
                }

                if let Some(swapchain_data) = self.swapchain_data.take() {
                    swapchain_data
                        .loader
                        .destroy_swapchain(swapchain_data.swapchain, None);
                }

                device.destroy_device(None);
            }

            if let Some(surface) = self.surface.take() {
                self.surface_loader.destroy_surface(surface, None);
            }
        }

        self.window = None;
        self.surface = None;
        self.device = None;
        self.graphics_queue = None;
        self.graphics_queue_index = None;
        self.swapchain_data = None; // Will probably be updated on the chapter swap chain recreation
        self.swapchain_image_views = None;
        self.command_buffers = None;
        self.vertex_staging_buffer = None;
        self.vertex_staging_buffer_memory = None;
        self.vertex_buffer = None;
        self.vertex_buffer_memory = None;
        self.index_staging_buffer = None;
        self.index_staging_buffer_memory = None;
        self.index_buffer = None;
        self.index_buffer_memory = None;
        self.uniform_buffers = None;
        self.uniform_buffer_memories = None;
        self.uniform_buffers_mapped = None;
        self.graphics_pipeline = None;
        self.descriptor_set_layout = None;
        self.descriptor_pool = None;
        self.descriptor_sets = None;
        self.pipeline_layout = None;
        self.command_pool = None;
        self.texture_image = None;
        self.texture_image_memory = None;
        self.texture_image_view = None;
        self.texture_image_sampler = None;
        self.descriptor_pool = None;
        self.present_complete_semaphores = None;
        self.render_finished_semaphores = None;
        self.in_flight_fences = None;
    }

    #[allow(unused)]
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
            WindowEvent::RedrawRequested => {
                #[allow(clippy::unwrap_used)]
                VulkanApp::draw_frame(
                    self.device.as_ref().unwrap(),
                    self.in_flight_fences.as_ref().unwrap(),
                    self.swapchain_data.as_ref().unwrap(),
                    self.present_complete_semaphores.as_ref().unwrap(),
                    self.render_finished_semaphores.as_ref().unwrap(),
                    self.command_buffers.as_ref().unwrap(),
                    self.swapchain_image_views.as_ref().unwrap(),
                    &self.graphics_pipeline.unwrap(),
                    &self.graphics_queue.unwrap(),
                    &[self.vertex_buffer.unwrap()],
                    &self.index_buffer.unwrap(),
                    &mut self.frame_index,
                    self.start_time,
                    &self.uniform_buffers_mapped.unwrap(),
                    &self.pipeline_layout.unwrap(),
                    self.descriptor_sets.as_ref().unwrap(),
                );
                // println!("fps: {}", self.fps_counter.tick());
                self.window.as_ref().unwrap().request_redraw();
            },
            _ => (),
        }
    }
}

impl VulkanApp {
    fn new(display_handle: &DisplayHandle) -> Self {
        let window = None;

        let entry = unsafe { Entry::load().expect("Failed to load Vulkan") };
        let required_extensions =
            ash_window::enumerate_required_extensions(display_handle.as_raw())
                .expect("Failed to retrieve required extensions from display handle");
        let instance = VulkanApp::create_instance(&entry, required_extensions);
        let (debug_utils_loader, debug_callback) = VulkanApp::setup_debug_utils(&entry, &instance);

        let surface = None;
        let surface_loader = surface::Instance::new(&entry, &instance);

        let physical_device = VulkanApp::pick_physical_device(&instance);
        let device = None;
        let graphics_queue = None;
        let graphics_queue_index = None;
        let swapchain_data = None;
        let swapchain_image_views = None;
        let graphics_pipeline = None;
        let descriptor_set_layout = None;
        let pipeline_layout = None;
        let command_pool = None;
        let texture_image = None;
        let texture_image_memory = None;
        let texture_image_view = None;
        let texture_image_sampler = None;
        let vertex_staging_buffer = None;
        let vertex_staging_buffer_memory = None;
        let vertex_buffer = None;
        let vertex_buffer_memory = None;
        let index_staging_buffer = None;
        let index_staging_buffer_memory = None;
        let index_buffer = None;
        let index_buffer_memory = None;
        let uniform_buffers = None;
        let uniform_buffer_memories = None;
        let uniform_buffers_mapped = None;
        let command_buffers = None;
        let descriptor_pool = None;
        let descriptor_sets = None;
        let present_complete_semaphores = None;
        let render_finished_semaphores = None;
        let in_flight_fences = None;
        let frame_index = 0;
        let fps_counter = utils::FpsCounter::new(100);
        let start_time = Instant::now();

        Self {
            window,
            surface,
            surface_loader,
            entry,
            instance,
            debug_utils_loader,
            debug_callback,
            physical_device,
            device,
            graphics_queue_index,
            graphics_queue,
            swapchain_data,
            swapchain_image_views,
            graphics_pipeline,
            descriptor_set_layout,
            pipeline_layout,
            command_pool,
            vertex_staging_buffer,
            vertex_staging_buffer_memory,
            vertex_buffer,
            vertex_buffer_memory,
            index_staging_buffer,
            index_staging_buffer_memory,
            index_buffer,
            index_buffer_memory,
            uniform_buffers,
            uniform_buffer_memories,
            uniform_buffers_mapped,
            command_buffers,
            descriptor_pool,
            descriptor_sets,
            texture_image,
            texture_image_memory,
            texture_image_view,
            texture_image_sampler,
            present_complete_semaphores,
            render_finished_semaphores,
            in_flight_fences,
            frame_index,
            fps_counter,
            start_time,
        }
    }

    fn validation_layers(entry: &Entry) -> Vec<*const c_char> {
        #[cfg(debug_assertions)]
        let layer_names = [c"VK_LAYER_KHRONOS_validation"];
        #[cfg(not(debug_assertions))]
        let layer_names = [];

        if !VulkanApp::check_validation_layer_support(entry, &layer_names) {
            panic!("Validation layers requested but not available");
        }

        let layers_names_raw: Vec<*const c_char> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();

        layers_names_raw
    }

    fn extension_names(entry: &Entry, required_extensions: &[*const i8]) -> Vec<*const i8> {
        let mut extension_names = required_extensions.to_vec();

        // Only needed for message callback in validation layers
        #[cfg(debug_assertions)]
        extension_names.push(debug_utils::NAME.as_ptr());

        if !VulkanApp::check_instance_extension_support(entry, required_extensions) {
            panic!("Extensions requested but not available");
        }

        extension_names
    }

    fn create_instance(entry: &Entry, required_extensions: &[*const i8]) -> ash::Instance {
        let app_name = CString::new(WINDOW_TITLE).unwrap();
        let engine_name = CString::new("Vulkan Engine").unwrap();

        let layer_names = VulkanApp::validation_layers(entry);

        let extension_names = VulkanApp::extension_names(entry, required_extensions);

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .engine_name(&engine_name)
            .application_version(1)
            .engine_version(1)
            .api_version(vk::make_api_version(0, 1, 4, 0));

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&extension_names);

        unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Failed to create vulkan instance")
        }
    }

    fn check_validation_layer_support(entry: &Entry, required_layers: &[&CStr]) -> bool {
        let mut layer_properties = unsafe {
            entry
                .enumerate_instance_layer_properties()
                .expect("Failed to retrieve instance layer properties")
        };

        if layer_properties.is_empty() {
            return false;
        }

        let layer_names: Vec<&CStr> = layer_properties
            .iter_mut()
            .map(|property| {
                property
                    .layer_name_as_c_str()
                    .expect("Failed to retrieve layer name as cstr")
            })
            .collect();

        for &layer in required_layers {
            if !layer_names.iter().any(|&l| *l == *layer) {
                return false;
            }
        }

        true
    }

    fn check_instance_extension_support(entry: &Entry, required_extensions: &[*const i8]) -> bool {
        let mut extension_properties = unsafe {
            entry
                .enumerate_instance_extension_properties(None)
                .expect("Failed to retrieve instance layer properties")
        };

        if extension_properties.is_empty() {
            return false;
        }

        let extension_names: Vec<&CStr> = extension_properties
            .iter_mut()
            .map(|property| {
                property
                    .extension_name_as_c_str()
                    .expect("Failed to retrieve layer name as cstr")
            })
            .collect();

        for &extension in required_extensions {
            if !extension_names
                .iter()
                .any(|&e| *e == unsafe { CStr::from_ptr(extension) })
            {
                return false;
            }
        }

        true
    }

    fn setup_debug_utils(
        entry: &Entry,
        instance: &Instance,
    ) -> (debug_utils::Instance, vk::DebugUtilsMessengerEXT) {
        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                    | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_utils_loader = debug_utils::Instance::new(entry, instance);
        let debug_call_back = unsafe {
            debug_utils_loader
                .create_debug_utils_messenger(&debug_info, None)
                .expect("Failed to create debug utils messenger")
        };

        (debug_utils_loader, debug_call_back)
    }

    fn pick_physical_device(instance: &Instance) -> vk::PhysicalDevice {
        let physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate physical devices")
        };

        if physical_devices.is_empty() {
            panic!("No GPU available");
        }

        for device in physical_devices {
            if VulkanApp::is_device_suitable(instance, &device) {
                return device;
            }
        }

        panic!("No device found with the required properties and features");
    }

    #[allow(unused)]
    // This score function has to be adapted to your own use case
    fn device_score(instance: &Instance, device: &vk::PhysicalDevice) -> u32 {
        let properties = unsafe { instance.get_physical_device_properties(*device) };

        let features = unsafe { instance.get_physical_device_features(*device) };
        let mut vk13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut vk11_features = vk::PhysicalDeviceVulkan11Features::default();
        let mut features2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut vk13_features)
            .push_next(&mut vk11_features);
        unsafe {
            instance.get_physical_device_features2(*device, &mut features2);
        }

        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(*device) };
        let device_extensions = unsafe {
            instance
                .enumerate_device_extension_properties(*device)
                .expect("Failed to retrieve device extensions properties")
        };

        // Determine if the device type is suitable
        let mut score = match properties.device_type {
            vk::PhysicalDeviceType::CPU | vk::PhysicalDeviceType::OTHER => 0,
            vk::PhysicalDeviceType::DISCRETE_GPU => 1000,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 10,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
            _ => unreachable!(),
        };

        let supports_vk_13 = properties.api_version >= vk::API_VERSION_1_3;

        // Check queue supports
        println!("\tSupport Queue Family: {}", queue_families.len());
        println!("\t\tQueue Count | Graphics, Compute, Transfer, Sparse Binding");
        let supports_graphics_queue = queue_families
            .iter()
            .any(|qf| qf.queue_flags.contains(vk::QueueFlags::GRAPHICS));

        // Check support for device extensions
        let required_extensions = [ash::khr::swapchain::NAME];
        let mut supports_required_extensions = true;
        for ext in required_extensions {
            supports_required_extensions = supports_required_extensions
                && device_extensions.iter().any(|e| {
                    *e.extension_name_as_c_str()
                        .expect("Failed to retrieve device extension name")
                        == *ext
                });
        }

        // Check support for required features
        let supports_required_features = features.geometry_shader == 1
            && vk13_features.dynamic_rendering == 1
            && vk11_features.shader_draw_parameters == 1;

        // The device isn't suitable
        if !(supports_vk_13
            && supports_graphics_queue
            && supports_required_extensions
            && supports_required_features
            && score > 0)
        {
            return 0;
        }

        // Increment the score for optional features, extensions, ..
        score += properties.limits.max_image_dimension2_d;

        score
    }

    fn is_device_suitable(instance: &Instance, device: &vk::PhysicalDevice) -> bool {
        let properties = unsafe { instance.get_physical_device_properties(*device) };

        let features = unsafe { instance.get_physical_device_features(*device) };
        let mut vk13_features = vk::PhysicalDeviceVulkan13Features::default();
        let mut vk11_features = vk::PhysicalDeviceVulkan11Features::default();
        let mut features2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut vk13_features)
            .push_next(&mut vk11_features);
        unsafe {
            instance.get_physical_device_features2(*device, &mut features2);
        }

        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(*device) };
        let device_extensions = unsafe {
            instance
                .enumerate_device_extension_properties(*device)
                .expect("Failed to retrieve device extensions properties")
        };

        // Determine if the device type is suitable
        let (device_type, type_suitable) = match properties.device_type {
            vk::PhysicalDeviceType::CPU => ("CPU", false),
            vk::PhysicalDeviceType::DISCRETE_GPU => ("Discrete GPU", true),
            vk::PhysicalDeviceType::INTEGRATED_GPU => ("Integrated GPU", true),
            vk::PhysicalDeviceType::VIRTUAL_GPU => ("Virtual GPU", true),
            vk::PhysicalDeviceType::OTHER => ("Other", false),
            _ => unreachable!(),
        };

        let device_name = vk_to_string(&properties.device_name);
        println!(
            "\tDevice Name: {}, id: {}, type: {}",
            device_name, properties.device_id, device_type
        );

        // Check if the device supports the right vulkan version
        let major = vk::api_version_major(properties.api_version);
        let minor = vk::api_version_minor(properties.api_version);
        let patch = vk::api_version_patch(properties.api_version);

        println!("\tAPI Version: {}.{}.{}", major, minor, patch);

        let supports_vk_13 = properties.api_version >= vk::API_VERSION_1_3;

        // Check queue supports
        println!("\tSupport Queue Family: {}", queue_families.len());
        println!("\t\tQueue Count | Graphics, Compute, Transfer, Sparse Binding");
        let mut supports_graphics_queue = false;
        for queue_family in queue_families.iter() {
            let is_graphics_support = if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            {
                "support"
            } else {
                "unsupport"
            };
            let is_compute_support = if queue_family.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                "support"
            } else {
                "unsupport"
            };
            let is_transfer_support = if queue_family.queue_flags.contains(vk::QueueFlags::TRANSFER)
            {
                "support"
            } else {
                "unsupport"
            };
            let is_sparse_support = if queue_family
                .queue_flags
                .contains(vk::QueueFlags::SPARSE_BINDING)
            {
                "support"
            } else {
                "unsupport"
            };

            println!(
                "\t\t{}\t    | {},  {},  {},  {}",
                queue_family.queue_count,
                is_graphics_support,
                is_compute_support,
                is_transfer_support,
                is_sparse_support
            );

            supports_graphics_queue = supports_graphics_queue || is_graphics_support == "support";
        }

        // Check support for device extensions
        let required_extensions = [ash::khr::swapchain::NAME];
        let mut supports_required_extensions_available = true;
        for ext in required_extensions {
            supports_required_extensions_available = supports_required_extensions_available
                && device_extensions.iter().any(|e| {
                    *e.extension_name_as_c_str()
                        .expect("Failed to retrieve device extension name")
                        == *ext
                });
        }

        // Check support for certain features
        let supports_required_features = features.geometry_shader == 1
            && vk13_features.dynamic_rendering == 1
            && vk13_features.synchronization2 == 1
            && vk11_features.shader_draw_parameters == 1
            && features.sampler_anisotropy == 1;

        type_suitable
            && supports_vk_13
            && supports_graphics_queue
            && supports_required_features
            && supports_required_extensions_available
    }

    fn create_logical_device(
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
        surface_loader: &surface::Instance,
        surface: &vk::SurfaceKHR,
    ) -> (ash::Device, u32) {
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(*physical_device) };

        let graphics_queue_index = unsafe {
            queue_families
            .iter()
            .enumerate()
            .position(|(index, qf)| qf.queue_flags.contains(vk::QueueFlags::GRAPHICS) && surface_loader.get_physical_device_surface_support(*physical_device, index as u32, *surface).expect("Failed to retrieve physical device surface support"))
            .expect(
                "Failed to get the graphics queue family index for the physical device selected",
            ) as u32
        };

        // Between [0.0, 1.0] - Influence the scheduling of command buffer execution
        // To create multiple queues, expend this array
        let queue_priorities = [0.5];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_index)
            .queue_priorities(&queue_priorities);

        let mut vk13_required_features = vk::PhysicalDeviceVulkan13Features::default()
            .dynamic_rendering(true)
            .synchronization2(true);
        let mut vk11_required_features =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
        let base_required_features = vk::PhysicalDeviceFeatures::default()
            .geometry_shader(true)
            .sampler_anisotropy(true);
        let mut required_features = vk::PhysicalDeviceFeatures2::default()
            .features(base_required_features)
            .push_next(&mut vk13_required_features)
            .push_next(&mut vk11_required_features);

        // Duplication in is_device_suitable. Acceptable for this tutorial but not recommended for real use case
        let required_device_extensions = [ash::khr::swapchain::NAME.as_ptr()];

        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info))
            .enabled_extension_names(&required_device_extensions)
            .push_next(&mut required_features);

        let device = unsafe {
            instance
                .create_device(*physical_device, &create_info, None)
                .expect("Failed to create logical device")
        };

        (device, graphics_queue_index)
    }

    fn create_surface(
        entry: &Entry,
        instance: &Instance,
        display_handle: &RawDisplayHandle,
        window: &Window,
    ) -> vk::SurfaceKHR {
        unsafe {
            ash_window::create_surface(
                entry,
                instance,
                *display_handle,
                window
                    .window_handle()
                    .expect("Failed to retrieve window_handle")
                    .as_raw(),
                None,
            )
            .expect("Failed to create the surface")
        }
    }

    fn create_swapchain(
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
        device: &ash::Device,
        surface_loader: &surface::Instance,
        surface: &vk::SurfaceKHR,
    ) -> SwapchainData {
        // Get details of swap chain support
        let surface_capabilities = unsafe {
            surface_loader
                .get_physical_device_surface_capabilities(*physical_device, *surface)
                .expect("Failed to retrieve surface capabilities")
        };

        let available_surface_formats = unsafe {
            surface_loader
                .get_physical_device_surface_formats(*physical_device, *surface)
                .expect("Failed to retrieve surface formats")
        };

        let available_present_modes = unsafe {
            surface_loader
                .get_physical_device_surface_present_modes(*physical_device, *surface)
                .expect("Failed to retrieve surface present modes")
        };

        let surface_format = VulkanApp::choose_swap_surface_format(&available_surface_formats);
        let present_mode = VulkanApp::choose_swap_present_mode(&available_present_modes);
        let extent = VulkanApp::choose_swap_extent(&surface_capabilities);
        let min_image_count = VulkanApp::choose_swap_min_image_count(&surface_capabilities);
        let pre_transform = surface_capabilities.current_transform;

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(*surface)
            .min_image_count(min_image_count)
            .image_color_space(surface_format.color_space)
            .image_format(surface_format.format)
            .image_extent(extent)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_array_layers(1);

        let swapchain_loader = swapchain::Device::new(instance, device);
        let swapchain = unsafe {
            swapchain_loader
                .create_swapchain(&create_info, None)
                .expect("Failed to create swapchain")
        };
        let images = unsafe {
            swapchain_loader
                .get_swapchain_images(swapchain)
                .expect("Failed to retrieve swapchain images")
        };

        SwapchainData {
            loader: swapchain_loader,
            swapchain,
            images,
            surface_format,
            extent,
        }
    }

    fn choose_swap_surface_format(
        available_surface_formats: &[vk::SurfaceFormatKHR],
    ) -> vk::SurfaceFormatKHR {
        assert!(
            !available_surface_formats.is_empty(),
            "Available surface format is empty"
        );

        // We look for SRGB color space and and the most common SRGB color format
        let format = available_surface_formats.iter().find(|&&f| {
            f.format == vk::Format::B8G8R8A8_SRGB
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        });

        *format.unwrap_or(&available_surface_formats[0])
    }

    fn choose_swap_present_mode(
        available_present_modes: &[vk::PresentModeKHR],
    ) -> vk::PresentModeKHR {
        // vk::PresentModeKHR::Fifo is guaranteed to be available
        let is_fifo_available = available_present_modes
            .iter()
            .find(|&&m| m == vk::PresentModeKHR::FIFO)
            .is_some();
        assert!(is_fifo_available);

        let mode = available_present_modes
            .iter()
            .find(|&&m| m == vk::PresentModeKHR::MAILBOX);

        *mode.unwrap_or(&vk::PresentModeKHR::FIFO)
    }

    fn choose_swap_extent(capabilities: &vk::SurfaceCapabilitiesKHR) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX {
            return capabilities.current_extent;
        }

        use num::clamp;

        vk::Extent2D {
            width: clamp(
                WINDOW_WIDTH,
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: clamp(
                WINDOW_HEIGHT,
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    }

    fn choose_swap_min_image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
        let mut min_image_count = std::cmp::max(3, capabilities.min_image_count);
        if 0 < capabilities.max_image_count && capabilities.max_image_count < min_image_count {
            min_image_count = capabilities.max_image_count;
        }

        min_image_count
    }

    fn create_image_views(
        device: &ash::Device,
        swapchain_data: &SwapchainData,
    ) -> Vec<vk::ImageView> {
        swapchain_data
            .images
            .iter()
            .map(|img| {
                VulkanApp::create_image_view(device, img, swapchain_data.surface_format.format)
            })
            .collect()
    }

    fn create_graphics_pipeline(
        device: &ash::Device,
        swapchain_data: &SwapchainData,
        descriptor_set_layout: &vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let shader_module = VulkanApp::create_shader_module(
            device,
            include_bytes!("../../../shaders/bin/23_texture_mapping.spv"),
        );
        let shader_stages = VulkanApp::get_pipeline_shader_stages_info(&shader_module);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let binding_descriptions = [Vertex::get_binding_description()];
        let attribute_descriptions = Vertex::get_attribute_descriptions();
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&attribute_descriptions)
            .vertex_binding_descriptions(&binding_descriptions);

        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        // Viewport and scissor are set to be dynamic so they'll be defined at draw time
        let _viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: swapchain_data.extent.width as f32,
            height: swapchain_data.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        let _scissor = vk::Rect2D::default()
            .extent(swapchain_data.extent)
            .offset(vk::Offset2D { x: 0, y: 0 });

        // We only define the count as viewport and scissor will be defined dynamically
        let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);

        let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false)
            .line_width(1.0);

        let multisample_info = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .sample_shading_enable(false);

        let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )];

        let color_blending_info = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&color_blend_attachments);

        let layouts = [*descriptor_set_layout];

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(&[]);

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .expect("Failed to create pipeline layout")
        };

        let color_attachment_formats = [swapchain_data.surface_format.format];

        let mut pipeline_rendering_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_attachment_formats);

        let graphics_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .viewport_state(&viewport_state_info)
            .rasterization_state(&rasterizer_info)
            .multisample_state(&multisample_info)
            .color_blend_state(&color_blending_info)
            .dynamic_state(&dynamic_state_info)
            .render_pass(vk::RenderPass::null())
            .layout(pipeline_layout)
            .push_next(&mut pipeline_rendering_info);

        let pipelines = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[graphics_pipeline_info],
                    None,
                )
                .expect("Failed to create graphics pipeline")
        };

        unsafe {
            #[allow(clippy::unwrap_used)]
            device.device_wait_idle().unwrap();
            device.destroy_shader_module(shader_module, None);
        }

        (pipelines[0], pipeline_layout)
    }

    fn create_shader_module(device: &ash::Device, shader_bytes: &[u8]) -> vk::ShaderModule {
        let mut cursor = Cursor::new(shader_bytes);
        let shader_code = read_spv(&mut cursor).expect("Failed to read shader spv file");

        let create_info = vk::ShaderModuleCreateInfo::default().code(&shader_code);

        unsafe {
            device
                .create_shader_module(&create_info, None)
                .expect("Failed to create shader module")
        }
    }

    fn get_pipeline_shader_stages_info(
        module: &vk::ShaderModule,
    ) -> Vec<vk::PipelineShaderStageCreateInfo<'_>> {
        let frag_shager_stage_create_info = vk::PipelineShaderStageCreateInfo::default()
            .module(*module)
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .name(c"fragMain");

        let vert_shager_stage_create_info = vk::PipelineShaderStageCreateInfo::default()
            .module(*module)
            .stage(vk::ShaderStageFlags::VERTEX)
            .name(c"vertMain");

        Vec::from([frag_shager_stage_create_info, vert_shager_stage_create_info])
    }

    fn create_command_pool(device: &ash::Device, graphics_queue_index: u32) -> vk::CommandPool {
        let create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(graphics_queue_index);

        unsafe {
            device
                .create_command_pool(&create_info, None)
                .expect("Failed to create command pool")
        }
    }

    fn create_command_buffers(
        device: &ash::Device,
        command_pool: &vk::CommandPool,
    ) -> Vec<vk::CommandBuffer> {
        let create_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(*command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(MAX_FRAMES_IN_FLIGHT);

        unsafe {
            device
                .allocate_command_buffers(&create_info)
                .expect("Failed to allocate command buffer")
        }
    }

    fn record_command_buffer(
        image_index: u32,
        device: &ash::Device,
        buffer: &vk::CommandBuffer,
        swapchain_data: &SwapchainData,
        swapchain_image_views: &[vk::ImageView],
        graphics_pipeline: &vk::Pipeline,
        vertex_buffers: &[vk::Buffer],
        index_buffer: &vk::Buffer,
        pipeline_layout: &vk::PipelineLayout,
        descriptor_sets: &[vk::DescriptorSet],
    ) {
        let create_info = vk::CommandBufferBeginInfo::default();
        unsafe {
            device
                .begin_command_buffer(*buffer, &create_info)
                .expect("Failed to begin buffer")
        };

        VulkanApp::transition_swapchain_image_layout(
            image_index,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::AccessFlags2::NONE,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            swapchain_data,
            buffer,
            device,
        );

        let clear_color = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        };

        let attachment_infos = [vk::RenderingAttachmentInfo::default()
            .image_view(swapchain_image_views[image_index as usize])
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_color)];

        let rendering_info = vk::RenderingInfo::default()
            .render_area(
                vk::Rect2D::default()
                    .extent(swapchain_data.extent)
                    .offset(vk::Offset2D { x: 0, y: 0 }),
            )
            .layer_count(1)
            .color_attachments(&attachment_infos);

        unsafe {
            device.cmd_begin_rendering(*buffer, &rendering_info);
        }

        unsafe {
            device.cmd_bind_pipeline(*buffer, vk::PipelineBindPoint::GRAPHICS, *graphics_pipeline)
        };

        unsafe {
            device.cmd_set_viewport(
                *buffer,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: swapchain_data.extent.width as f32,
                    height: swapchain_data.extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
        }

        unsafe {
            device.cmd_set_scissor(
                *buffer,
                0,
                &[vk::Rect2D::default()
                    .extent(swapchain_data.extent)
                    .offset(vk::Offset2D { x: 0, y: 0 })],
            );
        }

        unsafe { device.cmd_bind_vertex_buffers(*buffer, 0, vertex_buffers, &[0]) };
        unsafe {
            device.cmd_bind_index_buffer(*buffer, *index_buffer, 0, vk::IndexType::UINT16);
        }

        unsafe {
            device.cmd_bind_descriptor_sets(
                *buffer,
                vk::PipelineBindPoint::GRAPHICS,
                *pipeline_layout,
                0,
                descriptor_sets,
                &[],
            );
        }

        unsafe { device.cmd_draw_indexed(*buffer, INDICES.len() as u32, 1, 0, 0, 0) };

        unsafe { device.cmd_end_rendering(*buffer) };

        VulkanApp::transition_swapchain_image_layout(
            image_index,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags2::NONE,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            swapchain_data,
            buffer,
            device,
        );

        unsafe {
            device
                .end_command_buffer(*buffer)
                .expect("Failed to end the command buffer")
        };
    }

    #[allow(clippy::too_many_arguments)]
    fn transition_swapchain_image_layout(
        image_index: u32,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        src_access_mask: vk::AccessFlags2,
        dst_access_mask: vk::AccessFlags2,
        src_stage_mask: vk::PipelineStageFlags2,
        dst_stage_mask: vk::PipelineStageFlags2,
        swapchain_data: &SwapchainData,
        command_buffer: &vk::CommandBuffer,
        device: &ash::Device,
    ) {
        let barriers = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(src_stage_mask)
            .src_access_mask(src_access_mask)
            .dst_stage_mask(dst_stage_mask)
            .dst_access_mask(dst_access_mask)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_data.images[image_index as usize])
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })];

        let dependency_info = vk::DependencyInfo::default().image_memory_barriers(&barriers);

        unsafe { device.cmd_pipeline_barrier2(*command_buffer, &dependency_info) };
    }

    fn create_sync_objs(
        device: &ash::Device,
        swapchain_data: &SwapchainData,
    ) -> (Vec<vk::Semaphore>, Vec<vk::Semaphore>, Vec<vk::Fence>) {
        unsafe {
            let mut present_complete_semaphores = Vec::new();
            let mut render_finished_semaphores = Vec::new();
            let mut in_flight_fences = Vec::new();

            for _ in 0..swapchain_data.images.len() {
                render_finished_semaphores.push(
                    device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .expect("Failed to create semaphore"),
                );
            }

            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                present_complete_semaphores.push(
                    device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .expect("Failed to create semaphore"),
                );
                in_flight_fences.push(
                    device
                        .create_fence(
                            &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                            None,
                        )
                        .expect("Failed to create fence"),
                );
            }

            (
                present_complete_semaphores,
                render_finished_semaphores,
                in_flight_fences,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_frame(
        device: &ash::Device,
        in_flight_fences: &[vk::Fence],
        swapchain_data: &SwapchainData,
        present_complete_semaphores: &[vk::Semaphore],
        render_finished_semaphores: &[vk::Semaphore],
        buffers: &[vk::CommandBuffer],
        swapchain_image_views: &[vk::ImageView],
        graphics_pipeline: &vk::Pipeline,
        graphics_queue: &vk::Queue,
        vertex_buffers: &[vk::Buffer],
        index_buffer: &vk::Buffer,
        frame_index: &mut usize,
        start_time: Instant,
        uniform_buffers_mapped: &[*mut c_void; MAX_FRAMES_IN_FLIGHT as usize],
        pipeline_layout: &vk::PipelineLayout,
        descriptor_sets: &Vec<vk::DescriptorSet>,
    ) {
        unsafe {
            device
                .wait_for_fences(&[in_flight_fences[*frame_index]], true, u64::MAX)
                .expect("Failed to wait for fences");
            device
                .reset_fences(&[in_flight_fences[*frame_index]])
                .expect("Failed to reset fences");

            let (image_index, _result) = swapchain_data
                .loader
                .acquire_next_image(
                    swapchain_data.swapchain,
                    u64::MAX,
                    present_complete_semaphores[*frame_index],
                    vk::Fence::null(),
                )
                .expect("Failed to acquire next image");

            device
                .reset_command_buffer(
                    buffers[*frame_index],
                    vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                )
                .expect("Failed to reset command buffer");
            VulkanApp::record_command_buffer(
                image_index,
                device,
                &buffers[*frame_index],
                swapchain_data,
                swapchain_image_views,
                graphics_pipeline,
                vertex_buffers,
                index_buffer,
                pipeline_layout,
                &[descriptor_sets[*frame_index]],
            );

            VulkanApp::update_uniform_buffer(
                *frame_index,
                start_time,
                swapchain_data,
                uniform_buffers_mapped,
            );

            let wait_destination_stage_masks = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let wait_semaphores = [present_complete_semaphores[*frame_index]];
            let buffers = [buffers[*frame_index]];
            let signal_semaphores = [render_finished_semaphores[image_index as usize]];
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_destination_stage_masks)
                .command_buffers(&buffers)
                .signal_semaphores(&signal_semaphores);

            device
                .queue_submit(
                    *graphics_queue,
                    &[submit_info],
                    in_flight_fences[*frame_index],
                )
                .expect("Failed to submit queue");

            let swapchains = [swapchain_data.swapchain];
            let image_indices = [image_index];

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            swapchain_data
                .loader
                .queue_present(*graphics_queue, &present_info)
                .expect("Failed to present image");

            *frame_index = (*frame_index + 1) % MAX_FRAMES_IN_FLIGHT as usize;
        }
    }

    fn create_vertex_buffer(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
        command_pool: &vk::CommandPool,
        queue: &vk::Queue,
    ) -> [(vk::Buffer, vk::DeviceMemory); 2] {
        let buffer_size = (size_of::<Vertex>() * VERTICES.len()) as u64;

        let (staging_buffer, staging_buffer_memory) = VulkanApp::create_buffer(
            instance,
            device,
            physical_device,
            buffer_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );

        let data_ptr = unsafe {
            device
                .map_memory(
                    staging_buffer_memory,
                    0,
                    buffer_size,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("Failed to map staging buffer memory")
        };

        let mut data_align =
            unsafe { Align::new(data_ptr, align_of::<Vertex>() as u64, buffer_size) };
        data_align.copy_from_slice(&VERTICES);
        unsafe { device.unmap_memory(staging_buffer_memory) };

        let (vertex_buffer, vertex_buffer_memory) = VulkanApp::create_buffer(
            instance,
            device,
            physical_device,
            buffer_size,
            vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        VulkanApp::copy_buffer(
            device,
            command_pool,
            queue,
            &staging_buffer,
            &vertex_buffer,
            buffer_size,
        );

        [
            (staging_buffer, staging_buffer_memory),
            (vertex_buffer, vertex_buffer_memory),
        ]
    }

    fn create_index_buffer(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
        command_pool: &vk::CommandPool,
        queue: &vk::Queue,
    ) -> [(vk::Buffer, vk::DeviceMemory); 2] {
        let buffer_size = (size_of::<u16>() * INDICES.len()) as u64;

        let (staging_buffer, staging_buffer_memory) = VulkanApp::create_buffer(
            instance,
            device,
            physical_device,
            buffer_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );

        let data_ptr = unsafe {
            device
                .map_memory(
                    staging_buffer_memory,
                    0,
                    buffer_size,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("Failed to map staging buffer memory")
        };

        let mut data_align = unsafe { Align::new(data_ptr, align_of::<u16>() as u64, buffer_size) };
        data_align.copy_from_slice(&INDICES);
        unsafe { device.unmap_memory(staging_buffer_memory) };

        let (index_buffer, index_buffer_memory) = VulkanApp::create_buffer(
            instance,
            device,
            physical_device,
            buffer_size,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        VulkanApp::copy_buffer(
            device,
            command_pool,
            queue,
            &staging_buffer,
            &index_buffer,
            buffer_size,
        );

        [
            (staging_buffer, staging_buffer_memory),
            (index_buffer, index_buffer_memory),
        ]
    }

    fn find_memory_type(
        instance: &Instance,
        physical_device: &vk::PhysicalDevice,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> u32 {
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(*physical_device) };
        for i in 0..memory_properties.memory_type_count {
            if (type_filter & (1 << i) != 1)
                && (memory_properties.memory_types[i as usize].property_flags & properties
                    == properties)
            {
                return i;
            }
        }

        panic!("Failed to find a suitable memory type");
    }

    fn create_buffer(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .expect("Failed to create vertex buffer")
        };

        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let memory_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(VulkanApp::find_memory_type(
                instance,
                physical_device,
                memory_requirements.memory_type_bits,
                properties,
            ));

        let buffer_memory = unsafe {
            device
                .allocate_memory(&memory_allocate_info, None)
                .expect("Failed to allocate memory")
        };

        unsafe {
            device
                .bind_buffer_memory(buffer, buffer_memory, 0)
                .expect("Failed to bind vertex buffer memory")
        };

        (buffer, buffer_memory)
    }

    fn copy_buffer(
        device: &ash::Device,
        command_pool: &vk::CommandPool,
        queue: &vk::Queue,
        src_buffer: &vk::Buffer,
        dst_buffer: &vk::Buffer,
        size: u64,
    ) {
        let command_copy_buffer = VulkanApp::begin_single_time_commands(device, command_pool);

        unsafe {
            device.cmd_copy_buffer(
                command_copy_buffer,
                *src_buffer,
                *dst_buffer,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size,
                }],
            )
        };

        VulkanApp::end_single_time_commands(device, queue, &command_copy_buffer);
    }

    fn create_descriptor_set_layout(device: &ash::Device) -> vk::DescriptorSetLayout {
        let ubo_layout_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX);
        let combined_image_sampler_binding = vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);
        let bindings = [ubo_layout_binding, combined_image_sampler_binding];
        let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        unsafe {
            device
                .create_descriptor_set_layout(&create_info, None)
                .expect("Failed to create descriptor set layout")
        }
    }

    fn update_uniform_buffer(
        frame_index: usize,
        start_time: Instant,
        swapchain_data: &SwapchainData,
        uniform_buffers_mapped: &[*mut c_void; MAX_FRAMES_IN_FLIGHT as usize],
    ) {
        let current_time = Instant::now();
        let time = (current_time - start_time).as_secs_f32();

        let ubo = UniformBufferObject {
            model: Mat4::from_axis_angle(Vec3::Z, time * 90.0_f32.to_radians()),
            view: glam::camera::rh::view::look_at_mat4(
                Vec3::new(2.0, 2.0, 2.0),
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ),
            proj: glam::camera::rh::proj::vulkan::perspective(
                45.0_f32.to_radians(),
                swapchain_data.extent.width as f32 / swapchain_data.extent.height as f32,
                0.1,
                10.0,
            ),
        };

        let mut data_align = unsafe {
            Align::new(
                uniform_buffers_mapped[frame_index],
                align_of::<UniformBufferObject>() as u64,
                size_of::<UniformBufferObject>() as u64,
            )
        };
        data_align.copy_from_slice(&[ubo]);
    }

    fn create_uniform_buffers(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
    ) -> (
        [vk::Buffer; MAX_FRAMES_IN_FLIGHT as usize],
        [vk::DeviceMemory; MAX_FRAMES_IN_FLIGHT as usize],
        [*mut c_void; MAX_FRAMES_IN_FLIGHT as usize],
    ) {
        let mut uniform_buffers = [vk::Buffer::default(); MAX_FRAMES_IN_FLIGHT as usize];
        let mut uniform_buffer_memories =
            [vk::DeviceMemory::default(); MAX_FRAMES_IN_FLIGHT as usize];
        let mut uniform_buffers_mapped = [std::ptr::null_mut(); MAX_FRAMES_IN_FLIGHT as usize];

        let buffer_size = size_of::<UniformBufferObject>() as u64;
        for i in 0..(MAX_FRAMES_IN_FLIGHT as usize) {
            let (buffer, mem) = VulkanApp::create_buffer(
                instance,
                device,
                physical_device,
                buffer_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            );
            let mapped = unsafe {
                device
                    .map_memory(mem, 0, buffer_size, vk::MemoryMapFlags::empty())
                    .expect("Failed to map memory")
            };

            uniform_buffers[i] = buffer;
            uniform_buffer_memories[i] = mem;
            uniform_buffers_mapped[i] = mapped;
        }

        (
            uniform_buffers,
            uniform_buffer_memories,
            uniform_buffers_mapped,
        )
    }

    fn create_descriptor_pool(device: &ash::Device) -> vk::DescriptorPool {
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .descriptor_count(MAX_FRAMES_IN_FLIGHT)
                .ty(vk::DescriptorType::UNIFORM_BUFFER),
            vk::DescriptorPoolSize::default()
                .descriptor_count(MAX_FRAMES_IN_FLIGHT)
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER),
        ];

        let create_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .max_sets(MAX_FRAMES_IN_FLIGHT);

        unsafe {
            device
                .create_descriptor_pool(&create_info, None)
                .expect("Failed to create descriptor pool")
        }
    }

    fn create_descriptor_sets(
        device: &ash::Device,
        descriptor_pool: &vk::DescriptorPool,
        descriptor_set_layout: &vk::DescriptorSetLayout,
        uniform_buffers: &[vk::Buffer],
        sampler: &vk::Sampler,
        texture_image_view: &vk::ImageView,
    ) -> Vec<vk::DescriptorSet> {
        let layouts = [*descriptor_set_layout; MAX_FRAMES_IN_FLIGHT as usize];

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(*descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("Failed to allocate descriptor set info")
        };

        for i in 0..MAX_FRAMES_IN_FLIGHT as usize {
            let buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(uniform_buffers[i])
                .offset(0)
                .range(size_of::<UniformBufferObject>() as u64)];
            let image_info = [vk::DescriptorImageInfo::default()
                .sampler(*sampler)
                .image_view(*texture_image_view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&buffer_info)
                    .descriptor_count(1),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&image_info)
                    .descriptor_count(1),
            ];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };
        }

        descriptor_sets
    }

    fn create_texture_image(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
        queue: &vk::Queue,
        command_pool: &vk::CommandPool,
    ) -> (vk::Image, vk::DeviceMemory) {
        let image = image::load_from_memory(include_bytes!("../../../textures/texture.jpg"))
            .expect("Failed to load texture image")
            .to_rgba8();
        let (width, height) = image.dimensions();

        let image_data = image.into_raw();
        let image_size = (image_data.len() * size_of::<u8>()) as u64;

        let (staging_buffer, staging_buffer_memory) = VulkanApp::create_buffer(
            instance,
            device,
            physical_device,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        );

        let data_ptr = unsafe {
            device
                .map_memory(
                    staging_buffer_memory,
                    0,
                    image_size,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("Failed to map staging buffer memory")
        };

        let mut data_align = unsafe { Align::new(data_ptr, align_of::<u8>() as u64, image_size) };
        data_align.copy_from_slice(&image_data);
        unsafe { device.unmap_memory(staging_buffer_memory) };

        let (image, image_mem) = VulkanApp::create_image(
            instance,
            device,
            physical_device,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        );

        let command_buffer = VulkanApp::begin_single_time_commands(device, command_pool);
        VulkanApp::transition_image_layout(
            device,
            &command_buffer,
            &image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        VulkanApp::copy_buffer_to_image(
            device,
            width,
            height,
            &command_buffer,
            &staging_buffer,
            &image,
        );
        VulkanApp::transition_image_layout(
            device,
            &command_buffer,
            &image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        VulkanApp::end_single_time_commands(device, queue, &command_buffer);

        unsafe {
            device.free_memory(staging_buffer_memory, None);
            device.destroy_buffer(staging_buffer, None);
        }

        (image, image_mem)
    }

    fn create_image(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
        width: u32,
        height: u32,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> (vk::Image, vk::DeviceMemory) {
        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(tiling)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe {
            device
                .create_image(&image_create_info, None)
                .expect("Failed to create image")
        };
        let mem_requirements = unsafe { device.get_image_memory_requirements(image) };
        let alloc_info = vk::MemoryAllocateInfo::default()
            .memory_type_index(VulkanApp::find_memory_type(
                instance,
                physical_device,
                mem_requirements.memory_type_bits,
                properties,
            ))
            .allocation_size(mem_requirements.size);
        let image_mem = unsafe {
            device
                .allocate_memory(&alloc_info, None)
                .expect("Failed to allocate memory")
        };
        unsafe {
            device
                .bind_image_memory(image, image_mem, 0)
                .expect("Failed to bind image memory")
        };

        (image, image_mem)
    }

    fn begin_single_time_commands(
        device: &ash::Device,
        command_pool: &vk::CommandPool,
    ) -> vk::CommandBuffer {
        // For such short-lived buffers we could (and should) use a dedicated CommandPool with vk::CommandPoolCreateFlags::Transient
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(*command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = unsafe {
            device
                .allocate_command_buffers(&alloc_info)
                .expect("Failed to allocate command buffer")[0]
        };
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe {
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .expect("Failed to begin command buffer")
        };

        command_buffer
    }

    fn end_single_time_commands(
        device: &ash::Device,
        queue: &vk::Queue,
        command_buffer: &vk::CommandBuffer,
    ) {
        unsafe {
            device
                .end_command_buffer(*command_buffer)
                .expect("Failed to end command buffer");
            let command_buffers = [*command_buffer];
            let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
            device
                .queue_submit(*queue, &[submit_info], vk::Fence::null())
                .expect("Failed to submit queue");
            device
                .queue_wait_idle(*queue)
                .expect("Failed to wait queue idle");
        }
    }

    fn transition_image_layout(
        device: &ash::Device,
        command_buffer: &vk::CommandBuffer,
        image: &vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) {
        let mut barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(*image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        let src_stage_mask;
        let dst_stage_mask;

        if old_layout == vk::ImageLayout::UNDEFINED
            && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        {
            barrier = barrier.src_access_mask(vk::AccessFlags::empty());
            barrier = barrier.dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            src_stage_mask = vk::PipelineStageFlags::TOP_OF_PIPE;
            dst_stage_mask = vk::PipelineStageFlags::TRANSFER;
        } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
            && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        {
            barrier = barrier.src_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            barrier = barrier.dst_access_mask(vk::AccessFlags::SHADER_READ);

            src_stage_mask = vk::PipelineStageFlags::TRANSFER;
            dst_stage_mask = vk::PipelineStageFlags::FRAGMENT_SHADER;
        } else {
            panic!("Image layout transition invalid");
        }

        unsafe {
            device.cmd_pipeline_barrier(
                *command_buffer,
                src_stage_mask,
                dst_stage_mask,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            )
        };
    }

    fn copy_buffer_to_image(
        device: &ash::Device,
        width: u32,
        height: u32,
        command_buffer: &vk::CommandBuffer,
        buffer: &vk::Buffer,
        image: &vk::Image,
    ) {
        let regions = [vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_offset(vk::Offset3D::default())
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })];

        unsafe {
            device.cmd_copy_buffer_to_image(
                *command_buffer,
                *buffer,
                *image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
            );
        }
    }

    fn create_image_view(
        device: &ash::Device,
        image: &vk::Image,
        format: vk::Format,
    ) -> vk::ImageView {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        unsafe {
            device
                .create_image_view(&create_info, None)
                .expect("Failed to create image view")
        }
    }

    fn create_texture_image_view(device: &ash::Device, image: &vk::Image) -> vk::ImageView {
        VulkanApp::create_image_view(device, image, vk::Format::R8G8B8A8_SRGB)
    }

    fn create_texture_image_sampler(
        instance: &Instance,
        device: &ash::Device,
        physical_device: &vk::PhysicalDevice,
    ) -> vk::Sampler {
        let properties = unsafe { instance.get_physical_device_properties(*physical_device) };
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(properties.limits.max_sampler_anisotropy)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        unsafe {
            device
                .create_sampler(&sampler_info, None)
                .expect("Failed to create sampler")
        }
    }

    // The way I've implemented functions as VulkanApp:: methods makes it difficult to implement this step of the tutorial
    // It would require a full refacto, which would be necessary for a real use case
    // But here I want to keep the information on what object is needed for each functions easily to later improvements and optimisation that would be way beyond this tutorial
    // So I'm skipping this for now
    #[allow(unused)]
    fn recreate_swapchain() {}
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        unsafe {
            if let Some(device) = self.device.take() {
                #[allow(clippy::unwrap_used)]
                device.device_wait_idle().unwrap();

                if let Some(present_complete_semaphores) = self.present_complete_semaphores.take() {
                    for sem in present_complete_semaphores {
                        device.destroy_semaphore(sem, None);
                    }
                }

                if let Some(render_finished_semaphores) = self.render_finished_semaphores.take() {
                    for sem in render_finished_semaphores {
                        device.destroy_semaphore(sem, None);
                    }
                }

                if let Some(in_flight_fence) = self.in_flight_fences.take() {
                    for fence in in_flight_fence {
                        device.destroy_fence(fence, None);
                    }
                }

                if let Some(sampler) = self.texture_image_sampler.take() {
                    device.destroy_sampler(sampler, None);
                }

                if let Some(im_view) = self.texture_image_view.take() {
                    device.destroy_image_view(im_view, None);
                }

                if let Some(mem) = self.texture_image_memory.take() {
                    device.free_memory(mem, None);
                }

                if let Some(im) = self.texture_image.take() {
                    device.destroy_image(im, None);
                }

                if let Some(vertex_buffer_memory) = self.vertex_buffer_memory.take() {
                    device.free_memory(vertex_buffer_memory, None);
                }

                if let Some(vertex_staging_buffer_memory) = self.vertex_staging_buffer_memory.take()
                {
                    device.free_memory(vertex_staging_buffer_memory, None);
                }

                if let Some(vertex_buffer) = self.vertex_buffer.take() {
                    device.destroy_buffer(vertex_buffer, None);
                }

                if let Some(vertex_staging_buffer) = self.vertex_staging_buffer.take() {
                    device.destroy_buffer(vertex_staging_buffer, None);
                }

                if let Some(index_buffer_memory) = self.index_buffer_memory.take() {
                    device.free_memory(index_buffer_memory, None);
                }

                if let Some(index_staging_buffer_memory) = self.index_staging_buffer_memory.take() {
                    device.free_memory(index_staging_buffer_memory, None);
                }

                if let Some(uniform_buffer_memories) = self.uniform_buffer_memories.take() {
                    for mem in uniform_buffer_memories {
                        device.unmap_memory(mem);
                        device.free_memory(mem, None);
                    }
                }

                if let Some(index_buffer) = self.index_buffer.take() {
                    device.destroy_buffer(index_buffer, None);
                }

                if let Some(index_staging_buffer) = self.index_staging_buffer.take() {
                    device.destroy_buffer(index_staging_buffer, None);
                }

                if let Some(uniform_buffers) = self.uniform_buffers.take() {
                    for buf in uniform_buffers {
                        device.destroy_buffer(buf, None);
                    }
                }

                if let Some(layout) = self.descriptor_set_layout.take() {
                    device.destroy_descriptor_set_layout(layout, None);
                }

                if let Some(layout) = self.pipeline_layout.take() {
                    device.destroy_pipeline_layout(layout, None);
                }

                if let Some(pipeline) = self.graphics_pipeline.take() {
                    device.destroy_pipeline(pipeline, None);
                }

                if let Some(descriptor_pool) = self.descriptor_pool.take() {
                    device.destroy_descriptor_pool(descriptor_pool, None);
                }

                if let Some(image_views) = self.swapchain_image_views.take() {
                    for image_view in image_views {
                        device.destroy_image_view(image_view, None);
                    }
                }

                if let Some(pool) = self.command_pool.take() {
                    device.destroy_command_pool(pool, None);
                }

                if let Some(swapchain_data) = self.swapchain_data.take() {
                    swapchain_data
                        .loader
                        .destroy_swapchain(swapchain_data.swapchain, None);
                }

                device.destroy_device(None);
            }

            if let Some(surface) = self.surface.take() {
                self.surface_loader.destroy_surface(surface, None);
            }
            self.debug_utils_loader
                .destroy_debug_utils_messenger(self.debug_callback, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create EventLoop");

    // Continuously run the event loop. Ideal for games
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = VulkanApp::new(
        &event_loop
            .display_handle()
            .expect("Failed to retrieve display handle"),
    );

    event_loop
        .run_app(&mut app)
        .expect("Error occured during the event loop");
}
