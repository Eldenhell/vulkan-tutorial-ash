use std::{ffi::CStr, os::raw::c_char};

pub fn vk_to_string(raw_string_array: &[c_char]) -> String {
    let raw_string = unsafe { CStr::from_ptr(raw_string_array.as_ptr()) };

    raw_string
        .to_str()
        .expect("Failed to convert vulkan raw string")
        .to_owned()
}

use std::collections::VecDeque;
use std::time::Instant;

pub struct FpsCounter {
    frame_times: VecDeque<f64>,
    last_frame: Instant,
    max_samples: usize,
}

impl FpsCounter {
    pub fn new(max_samples: usize) -> Self {
        Self {
            frame_times: VecDeque::with_capacity(max_samples),
            last_frame: Instant::now(),
            max_samples,
        }
    }

    pub fn tick(&mut self) -> f64 {
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;

        self.frame_times.push_back(delta);
        if self.frame_times.len() > self.max_samples {
            self.frame_times.pop_front();
        }

        let avg_delta = self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64;
        if avg_delta > 0.0 {
            1.0 / avg_delta
        } else {
            0.0
        }
    }
}
