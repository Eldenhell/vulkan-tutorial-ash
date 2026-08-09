use std::{ffi::CStr, os::raw::c_char};

pub fn vk_to_string(raw_string_array: &[c_char]) -> String {
    let raw_string = unsafe { CStr::from_ptr(raw_string_array.as_ptr()) };

    raw_string
        .to_str()
        .expect("Failed to convert vulkan raw string")
        .to_owned()
}
