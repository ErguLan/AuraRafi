use std::ptr::NonNull;

use core_graphics_types::geometry::{CGRect, CGSize};
use metal::{DeviceRef, MTLPixelFormat, MetalLayer};
use objc::{msg_send, runtime::Object, sel, sel_impl};
use raw_window_handle::RawWindowHandle;

use crate::{Error, Result};

pub(super) const FORMAT: MTLPixelFormat = MTLPixelFormat::BGRA8Unorm;

pub(super) struct WindowLayer {
    pub layer: MetalLayer,
    view: NonNull<Object>,
}

impl WindowLayer {
    /// The caller keeps the view valid and uses the UI thread.
    pub unsafe fn attach(handle: RawWindowHandle, device: &DeviceRef) -> Result<Self> {
        let view = match handle {
            #[cfg(target_os = "macos")]
            RawWindowHandle::AppKit(handle) => handle.ns_view.cast::<Object>(),
            #[cfg(target_os = "ios")]
            RawWindowHandle::UiKit(handle) => handle.ui_view.cast::<Object>(),
            _ => return Err(Error::Invalid("Metal requires an AppKit or UIKit view")),
        };
        unsafe {
            #[cfg(target_os = "macos")]
            {
                let _: () = msg_send![view.as_ptr(), setWantsLayer: objc::runtime::YES];
            }
            let parent: *mut Object = msg_send![view.as_ptr(), layer];
            if parent.is_null() {
                return Err(Error::SurfaceLost);
            }
            let layer = MetalLayer::new();
            layer.set_device(device);
            layer.set_pixel_format(FORMAT);
            layer.set_framebuffer_only(true);
            layer.set_opaque(true);
            let _: () = msg_send![parent, addSublayer: &*layer];
            Ok(Self { layer, view })
        }
    }

    pub fn configure(&self, extent: [u32; 2]) {
        unsafe {
            let mut bounds: CGRect = msg_send![self.view.as_ptr(), bounds];
            bounds.origin.x = 0.;
            bounds.origin.y = 0.;
            let _: () = msg_send![&*self.layer, setFrame: bounds];
            if bounds.size.width > 0. {
                self.layer
                    .set_contents_scale(f64::from(extent[0]) / bounds.size.width);
            }
        }
        self.layer
            .set_drawable_size(CGSize::new(f64::from(extent[0]), f64::from(extent[1])));
    }
}

impl Drop for WindowLayer {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&*self.layer, removeFromSuperlayer];
        }
    }
}
