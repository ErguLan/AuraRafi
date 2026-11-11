//! Native accessibility is an on-demand projection of retained RafUI surfaces.
mod provider;
mod snapshot;

use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use raf_core::Language;
use raf_render::api_graphic_basic::EditorUiLayer;
use winit::{event_loop::EventLoopProxy, window::Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{Foundation::*, UI::{Accessibility::*, Shell::*, WindowsAndMessaging::*}, Graphics::Gdi::ClientToScreen, System::Threading::GetCurrentThreadId};
use snapshot::{Element, Snapshot};

const SUBCLASS_ID: usize = 0x524146;
const CLIENT_GRACE_MS: u64 = 10_000;

struct Shared {
    enabled: AtomicBool, hwnd: AtomicIsize, ui_thread: AtomicU32,
    snapshot: RwLock<Arc<Snapshot>>, wakeup: EventLoopProxy<()>,
    started: Instant, last_query: AtomicU64,
}
impl Shared {
    pub fn touch(&self) {
        let now = self.started.elapsed().as_millis() as u64 + 1;
        let old = self.last_query.swap(now, Ordering::Relaxed);
        if old == 0 || now.saturating_sub(old) > CLIENT_GRACE_MS { let _ = self.wakeup.send_event(()); }
    }
    fn active(&self) -> bool {
        let last = self.last_query.load(Ordering::Relaxed);
        self.enabled.load(Ordering::Acquire) && last > 0
            && (self.started.elapsed().as_millis() as u64 + 1).saturating_sub(last) <= CLIENT_GRACE_MS
    }
}
pub(crate) struct NativeAccessibility {
    shared: Arc<Shared>, last_signature: Option<u64>,
    retained_frames: Vec<Arc<raf_render::api_graphic_basic::ui_surface::UiSurfaceFrame>>,
}
impl NativeAccessibility {
    pub fn new(wakeup: EventLoopProxy<()>) -> Self {
        Self { shared: Arc::new(Shared { enabled: AtomicBool::new(false), hwnd: AtomicIsize::new(0), ui_thread: AtomicU32::new(0),
            snapshot: RwLock::new(Arc::new(Snapshot::new(Element::root(String::new(), [0.0; 4])))),
            wakeup, started: Instant::now(), last_query: AtomicU64::new(0) }), last_signature: None, retained_frames: Vec::new() }
    }
    pub fn attach(&mut self, window: &Window) {
        let Ok(handle) = window.window_handle() else { return; };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else { return; };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        let shared = Arc::into_raw(self.shared.clone());
        let installed = unsafe { SetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, shared as usize).as_bool() };
        if installed {
            self.shared.hwnd.store(hwnd.0 as isize, Ordering::Release);
            self.shared.ui_thread.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        } else { unsafe { drop(Arc::from_raw(shared)); } }
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.shared.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.last_signature = None;
            self.retained_frames.clear();
            self.shared.last_query.store(0, Ordering::Relaxed);
            if !enabled {
                if let Ok(mut snapshot) = self.shared.snapshot.write() {
                    for node in &snapshot.elements { if let Some(inbox) = &node.inbox { inbox.cancel(); } }
                    let root = snapshot.elements[0].clone();
                    *snapshot = Arc::new(Snapshot::new(Element::root(root.name, root.bounds)));
                }
            }
        }
    }
    pub fn publish(&mut self, window: &Window, layers: &mut [EditorUiLayer<'_>], exclusive_last: bool,
        language: Language, mut resolve: impl FnMut(&str) -> String) {
        if !self.shared.active() { return; }
        let hwnd = HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut _);
        if hwnd.0.is_null() { return; }
        let mut origin = POINT::default();
        if !unsafe { ClientToScreen(hwnd, &mut origin).as_bool() } { return; }
        let size = window.inner_size();
        let mut signature = std::collections::hash_map::DefaultHasher::new();
        (origin.x, origin.y, size.width, size.height, exclusive_last, format!("{language:?}")).hash(&mut signature);
        let skip = if exclusive_last { layers.len().saturating_sub(1) } else { 0 };
        let mut frames = Vec::with_capacity(layers.len().saturating_sub(skip));
        for layer in layers.iter().skip(skip) {
            let frame = layer.host.accessibility_frame();
            frame.as_ref().map(|frame| Arc::as_ptr(frame) as usize).hash(&mut signature);
            if let Some(frame) = frame { frames.push(frame); }
            (layer.target_rect.x, layer.target_rect.y, layer.target_rect.width, layer.target_rect.height).hash(&mut signature);
        }
        let signature = signature.finish();
        if self.last_signature == Some(signature) { return; }
        // Retain the identities used in the signature, preventing allocator reuse
        // from making changed geometry appear unchanged on a later presentation.
        self.retained_frames = frames;
        let mut next = Snapshot::new(Element::root(window.title(), [origin.x as f64, origin.y as f64, size.width as f64, size.height as f64]));
        for layer in layers.iter_mut().skip(skip) { next.add_layer(layer, [origin.x as f64, origin.y as f64], language, &mut resolve); }
        let next = Arc::new(next);
        let old = {
            let Ok(mut current) = self.shared.snapshot.write() else { return; };
            std::mem::replace(&mut *current, next.clone())
        };
        // Expire requests belonging to removed or covered surfaces immediately.
        let mut active_queues: Vec<&raf_ui::automation::UiAutomationInbox> = Vec::new();
        for node in &next.elements {
            if let Some(inbox) = &node.inbox {
                if !active_queues.iter().any(|other| inbox.same_queue(other)) { active_queues.push(inbox); }
            }
        }
        for element in &old.elements {
            // A disappearing menu item must not cancel requests to other
            // controls on the same surface. Individual stale targets are
            // rejected against the current document when the queue drains.
            if let Some(inbox) = &element.inbox {
                if !active_queues.iter().any(|other| inbox.same_queue(other)) {
                    inbox.cancel();
                }
            }
        }
        self.last_signature = Some(signature);
        if unsafe { UiaClientsAreListening().as_bool() } {
            let root = provider::element(self.shared.clone(), "rafui.window".into());
            if old.indexes.keys().any(|key| !next.indexes.contains_key(key)) || old.elements.len() != next.elements.len() {
                let _ = unsafe { UiaRaiseStructureChangedEvent(&root, StructureChangeType_ChildrenInvalidated, std::ptr::null_mut(), 0) };
            }
            let old_focus = old.elements.iter().find(|element| element.focused).map(|element| &element.key);
            if let Some(focused) = next.elements.iter().find(|element| element.focused) {
                if old_focus != Some(&focused.key) {
                    let item = provider::element(self.shared.clone(), focused.key.clone());
                    let _ = unsafe { UiaRaiseAutomationEvent(&item, UIA_AutomationFocusChangedEventId) };
                }
            }
            for node in &next.elements {
                let Some(previous) = old.indexes.get(&node.key).map(|index| &old.elements[*index]) else { continue; };
                if node.checked != previous.checked || (node.value != previous.value && !node.password) {
                    let item = provider::element(self.shared.clone(), node.key.clone());
                    if node.checked != previous.checked {
                        let before = i32::from(previous.checked.unwrap_or(false));
                        let after = i32::from(node.checked.unwrap_or(false));
                        let _ = unsafe { UiaRaiseAutomationPropertyChangedEvent(&item, UIA_ToggleToggleStatePropertyId,
                            &windows::core::VARIANT::from(before), &windows::core::VARIANT::from(after)) };
                    }
                    if node.value != previous.value && !node.password {
                        let _ = unsafe { UiaRaiseAutomationPropertyChangedEvent(&item, UIA_ValueValuePropertyId,
                            &windows::core::VARIANT::from(previous.value.as_deref().unwrap_or("")),
                            &windows::core::VARIANT::from(node.value.as_deref().unwrap_or(""))) };
                    }
                }
            }
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, data: usize) -> LRESULT {
    let ptr = data as *const Shared;
    let shared = &*ptr;
    if message == WM_GETOBJECT && lparam.0 as i32 == UiaRootObjectId && shared.enabled.load(Ordering::Acquire) {
        shared.touch();
        Arc::increment_strong_count(ptr);
        let root = provider::element(Arc::from_raw(ptr), "rafui.window".into());
        return UiaReturnRawElementProvider(hwnd, wparam, lparam, &root);
    }
    if message == WM_NCDESTROY {
        shared.enabled.store(false, Ordering::Release);
        shared.hwnd.store(0, Ordering::Release);
        let _ = RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
        let result = DefSubclassProc(hwnd, message, wparam, lparam);
        drop(Arc::from_raw(ptr));
        return result;
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}
