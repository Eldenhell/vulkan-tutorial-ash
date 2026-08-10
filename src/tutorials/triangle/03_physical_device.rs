use std::{
    borrow::Cow,
    ffi::{self, CStr, CString},
    os::raw::c_char,
};

use ash::{Entry, Instance, ext::debug_utils, vk};
use vulkan_tuto_ash::utils::vk_to_string;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowAttributes},
};

const WINDOW_TITLE: &str = "03. Physical device";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

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

struct VulkanApp {
    window: Option<Window>,

    entry: Entry,
    instance: Instance,
    debug_utils_loader: debug_utils::Instance,
    debug_callback: vk::DebugUtilsMessengerEXT,

    physical_device: vk::PhysicalDevice,
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
        let (debug_utils_loader, debug_callback) = VulkanApp::setup_debug_utils(&entry, &instance);
        let physical_device = VulkanApp::pick_physical_device(&instance);

        Self {
            window,
            entry,
            instance,
            debug_utils_loader,
            debug_callback,
            physical_device,
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
        let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk13_features);
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
        let supports_required_features =
            features.geometry_shader == 1 && vk13_features.dynamic_rendering == 1;

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
        let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk13_features);
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
        let supports_required_features =
            features.geometry_shader == 1 && vk13_features.dynamic_rendering == 1;

        type_suitable
            && supports_vk_13
            && supports_graphics_queue
            && supports_required_features
            && supports_required_extensions_available
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        unsafe {
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
        ash_window::enumerate_required_extensions(
            event_loop
                .display_handle()
                .expect("Failed to retrieve display handle")
                .as_raw(),
        )
        .expect("Failed to retrieve required extensions from display handle"),
    );
    event_loop
        .run_app(&mut app)
        .expect("Error occured during the event loop");
}
