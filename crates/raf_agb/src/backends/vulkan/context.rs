use std::{ffi::CStr, rc::Rc};

use ash::{khr, vk, Entry, Instance};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{AdapterInfo, Error, Result};

use super::vk_error;

pub(super) struct InstanceOwner {
    raw: Instance,
    _entry: Entry,
}

impl Drop for InstanceOwner {
    fn drop(&mut self) {
        unsafe { self.raw.destroy_instance(None) };
    }
}

pub(super) struct Surface {
    pub api: khr::surface::Instance,
    pub raw: vk::SurfaceKHR,
    _instance: Rc<InstanceOwner>,
}

impl Surface {
    pub fn choose_format(&self, adapter: vk::PhysicalDevice) -> Result<vk::SurfaceFormatKHR> {
        let formats = unsafe {
            self.api
                .get_physical_device_surface_formats(adapter, self.raw)
        }
        .map_err(vk_error)?;
        [vk::Format::B8G8R8A8_UNORM, vk::Format::R8G8B8A8_UNORM]
            .into_iter()
            .find_map(|format| {
                formats
                    .iter()
                    .find(|entry| {
                        (entry.format == format || entry.format == vk::Format::UNDEFINED)
                            && entry.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    })
                    .map(|entry| vk::SurfaceFormatKHR {
                        format,
                        color_space: entry.color_space,
                    })
            })
            .ok_or_else(|| Error::Backend("Vulkan surface lacks an 8-bit SDR format".into()))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { self.api.destroy_surface(self.raw, None) };
    }
}

pub(super) struct Gpu {
    pub instance: Rc<InstanceOwner>,
    pub device: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub family: u32,
    pub queue: vk::Queue,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    pub timestamp_bits: u32,
    pub timestamp_period: f32,
}

impl Gpu {
    pub fn supports_sampled_color(&self, format: vk::Format) -> bool {
        unsafe {
            self.instance
                .raw
                .get_physical_device_format_properties(self.physical, format)
        }
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::COLOR_ATTACHMENT)
    }

    pub fn supports_sampled_depth(&self, format: vk::Format) -> bool {
        unsafe {
            self.instance
                .raw
                .get_physical_device_format_properties(self.physical, format)
        }
        .optimal_tiling_features
        .contains(
            vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )
    }

    pub fn swapchain_api(&self) -> khr::swapchain::Device {
        khr::swapchain::Device::new(&self.instance.raw, &self.device)
    }

    pub fn memory_type(&self, bits: u32, required: vk::MemoryPropertyFlags) -> Result<u32> {
        (0..self.memory.memory_type_count)
            .find(|&index| {
                bits & (1 << index) != 0
                    && self.memory.memory_types[index as usize]
                        .property_flags
                        .contains(required)
            })
            .ok_or_else(|| Error::Backend("Vulkan adapter lacks the required memory type".into()))
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe { self.device.destroy_device(None) };
    }
}

pub(super) fn open(
    window: Option<(RawDisplayHandle, RawWindowHandle)>,
) -> Result<(Rc<Gpu>, Option<Surface>, AdapterInfo)> {
    let entry = unsafe { Entry::load() }.map_err(|error| Error::Backend(error.to_string()))?;
    let app = vk::ApplicationInfo::default()
        .application_name(c"ApiGraphicBasic")
        .api_version(vk::API_VERSION_1_1);
    let extensions = match window {
        Some((display, _)) => {
            ash_window::enumerate_required_extensions(display).map_err(vk_error)?
        }
        None => &[],
    };
    let raw = unsafe {
        entry.create_instance(
            &vk::InstanceCreateInfo::default()
                .application_info(&app)
                .enabled_extension_names(extensions),
            None,
        )
    }
    .map_err(vk_error)?;
    let instance = Rc::new(InstanceOwner { raw, _entry: entry });
    let surface = if let Some((display, window)) = window {
        let api = khr::surface::Instance::new(&instance._entry, &instance.raw);
        let raw = unsafe {
            ash_window::create_surface(&instance._entry, &instance.raw, display, window, None)
        }
        .map_err(vk_error)?;
        Some(Surface {
            api,
            raw,
            _instance: Rc::clone(&instance),
        })
    } else {
        None
    };
    let (physical, family, properties, queue_properties) =
        choose_adapter(&instance.raw, surface.as_ref())?;
    let priority = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priority)];
    let swapchain = [khr::swapchain::NAME.as_ptr()];
    let extensions = if surface.is_some() {
        &swapchain[..]
    } else {
        &[]
    };
    let info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(extensions);
    let device = unsafe { instance.raw.create_device(physical, &info, None) }.map_err(vk_error)?;
    let queue = unsafe { device.get_device_queue(family, 0) };
    let memory = unsafe { instance.raw.get_physical_device_memory_properties(physical) };
    let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let timestamp_bits = queue_properties.timestamp_valid_bits;
    let info = AdapterInfo {
        backend: "Vulkan",
        name,
        driver: format!("{:#010x}", properties.driver_version),
        hardware: properties.device_type != vk::PhysicalDeviceType::CPU,
        max_texture: properties.limits.max_image_dimension2_d,
        max_buffer: u32::MAX.into(),
        gpu_timestamps: timestamp_bits != 0,
    };
    Ok((
        Rc::new(Gpu {
            instance,
            device,
            physical,
            family,
            queue,
            memory,
            timestamp_bits,
            timestamp_period: properties.limits.timestamp_period,
        }),
        surface,
        info,
    ))
}

fn choose_adapter(
    instance: &Instance,
    surface: Option<&Surface>,
) -> Result<(
    vk::PhysicalDevice,
    u32,
    vk::PhysicalDeviceProperties,
    vk::QueueFamilyProperties,
)> {
    let mut best = None;
    let mut best_rank = u8::MAX;
    for physical in unsafe { instance.enumerate_physical_devices() }.map_err(vk_error)? {
        let props = unsafe { instance.get_physical_device_properties(physical) };
        if props.api_version < vk::API_VERSION_1_1 {
            continue;
        }
        let rank = match props.device_type {
            vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
            vk::PhysicalDeviceType::DISCRETE_GPU => 1,
            vk::PhysicalDeviceType::CPU => 3,
            _ => 2,
        };
        if rank >= best_rank {
            continue;
        }
        if let Some(surface) = surface {
            let extensions = unsafe { instance.enumerate_device_extension_properties(physical) }
                .map_err(vk_error)?;
            if !extensions.iter().any(|ext| unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) } == khr::swapchain::NAME)
                || surface.choose_format(physical).is_err() { continue; }
        }
        for (index, queue) in
            unsafe { instance.get_physical_device_queue_family_properties(physical) }
                .into_iter()
                .enumerate()
        {
            if !queue.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                continue;
            }
            if let Some(surface) = surface {
                if !unsafe {
                    surface.api.get_physical_device_surface_support(
                        physical,
                        index as u32,
                        surface.raw,
                    )
                }
                .map_err(vk_error)?
                {
                    continue;
                }
            }
            best = Some((physical, index as u32, props, queue));
            best_rank = rank;
            break;
        }
    }
    best.ok_or_else(|| Error::Backend("no Vulkan 1.1 graphics adapter for this target".into()))
}
