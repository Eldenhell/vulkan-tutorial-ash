use std::{
    borrow::Cow,
    ffi::{self, CStr, CString},
    io::Cursor,
    mem::{offset_of, size_of},
    os::raw::c_char,
};

use ash::{
    Entry, Instance,
    ext::debug_utils,
    khr::{surface, swapchain},
    util::read_spv,
    vk,
};
use vulkan_tuto_ash::utils::vk_to_string;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    raw_window_handle::{DisplayHandle, HasDisplayHandle, HasWindowHandle, RawDisplayHandle},
    window::{Window, WindowAttributes},
};

const WINDOW_TITLE: &str = "15. Vertex buffer input description";
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

const VERTICES: [Vertex; 3] = [
    Vertex {
        pos: [0.0, -5.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        pos: [0.5, 0.5],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        pos: [-0.5, 0.5],
        color: [0.0, 0.0, 1.0],
    },
];

#[derive(Clone, Debug, Copy)]
struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 3],
}

impl Vertex {
    fn get_binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<Vertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 2] {
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
        ]
    }
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

    command_pool: Option<vk::CommandPool>,
    command_buffers: Option<Vec<vk::CommandBuffer>>,

    present_complete_semaphores: Option<Vec<vk::Semaphore>>,
    render_finished_semaphores: Option<Vec<vk::Semaphore>>,
    in_flight_fences: Option<Vec<vk::Fence>>,

    frame_index: usize,
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

        let graphics_pipeline = VulkanApp::create_graphics_pipeline(&device, &swapchain_data);

        let command_pool = VulkanApp::create_command_pool(&device, graphics_queue_index);

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
        self.command_pool = Some(command_pool);
        self.command_buffers = Some(command_buffers);
        self.present_complete_semaphores = Some(present_complete_semaphores);
        self.render_finished_semaphores = Some(render_finished_semaphores);
        self.in_flight_fences = Some(in_flight_fences);
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

                if let Some(pipeline) = self.graphics_pipeline.take() {
                    device.destroy_pipeline(pipeline, None);
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
        self.graphics_pipeline = None;
        self.command_pool = None;
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
                    &mut self.frame_index,
                );
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
        let command_pool = None;
        let command_buffers = None;
        let present_complete_semaphores = None;
        let render_finished_semaphores = None;
        let in_flight_fences = None;
        let frame_index = 0;

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
            command_pool,
            command_buffers,
            present_complete_semaphores,
            render_finished_semaphores,
            in_flight_fences,
            frame_index,
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
            && vk11_features.shader_draw_parameters == 1;

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
        let base_required_features = vk::PhysicalDeviceFeatures::default().geometry_shader(true);
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
        let create_info = vk::ImageViewCreateInfo::default()
            .format(swapchain_data.surface_format.format)
            .view_type(vk::ImageViewType::TYPE_2D)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            })
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        swapchain_data
            .images
            .iter()
            .map(|&img| {
                let create_info = create_info.image(img);
                unsafe {
                    device
                        .create_image_view(&create_info, None)
                        .expect("Failed to create image view")
                }
            })
            .collect()
    }

    fn create_graphics_pipeline(
        device: &ash::Device,
        swapchain_data: &SwapchainData,
    ) -> vk::Pipeline {
        let shader_module = VulkanApp::create_shader_module(
            device,
            include_bytes!("../../../shaders/bin/08_base_shaders.spv"),
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
            .front_face(vk::FrontFace::CLOCKWISE)
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

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&[])
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
            device.destroy_pipeline_layout(pipeline_layout, None);
            device.destroy_shader_module(shader_module, None);
        }

        pipelines[0]
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
    ) {
        let create_info = vk::CommandBufferBeginInfo::default();
        unsafe {
            device
                .begin_command_buffer(*buffer, &create_info)
                .expect("Failed to begin buffer")
        };

        VulkanApp::transition_image_layout(
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
            device.cmd_bind_pipeline(*buffer, vk::PipelineBindPoint::GRAPHICS, *graphics_pipeline);
        }

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

        unsafe { device.cmd_draw(*buffer, 3, 1, 0, 0) };

        unsafe { device.cmd_end_rendering(*buffer) };

        VulkanApp::transition_image_layout(
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
    fn transition_image_layout(
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
        frame_index: &mut usize,
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

                if let Some(pipeline) = self.graphics_pipeline.take() {
                    device.destroy_pipeline(pipeline, None);
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
