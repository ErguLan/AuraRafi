//! Native application-menu adapter boundary.
//!
//! RafUI keeps the command tree renderer-agnostic. A platform shell owns the
//! concrete menu API and returns only stable command activations to the app.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
#[cfg(target_os = "windows")]
use std::{ffi::c_void, mem::size_of};

use raf_ui::{UiApplicationMenu, UiMenuActivation, UiMenuItem};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Platform-specific bridge for an application menu bar.
///
/// Implementors may use the native Windows, Linux, or macOS menu facility,
/// but they must not invoke editor, scene, CAD, or persistence code. The
/// application receives `UiMenuActivation` values and dispatches them through
/// its one command boundary.
pub trait NativeApplicationMenuAdapter {
    fn install(&mut self, window: &Window, menu: &UiApplicationMenu) -> Result<(), String>;

    fn install_localized(
        &mut self,
        window: &Window,
        menu: &UiApplicationMenu,
        _resolve: &mut dyn FnMut(&str) -> String,
    ) -> Result<(), String> {
        self.install(window, menu)
    }

    fn drain_activations(&mut self) -> Vec<UiMenuActivation>;
}

/// Native application-menu adapter for a Winit window.
///
/// Windows owns the menu bar and sends `WM_COMMAND` to the window. The
/// subclass below translates those command IDs into the renderer-neutral
/// `UiMenuActivation` channel. Other platforms remain explicit extension
/// points until their native shell is connected to this same model.
#[derive(Default)]
pub struct NativeWindowApplicationMenuAdapter {
    window_handle: Option<RawWindowHandle>,
    signature: Option<MenuSignature>,
    command_ids: HashSet<String>,
    activations: Option<Receiver<UiMenuActivation>>,
    #[cfg(target_os = "windows")]
    native_menu: Option<WindowsNativeMenu>,
}

#[derive(Clone, PartialEq, Eq)]
struct MenuSignature {
    menu: UiApplicationMenu,
    labels: Vec<String>,
}

impl NativeWindowApplicationMenuAdapter {
    pub fn is_installed(&self) -> bool {
        self.window_handle.is_some() && self.signature.is_some() && self.activations.is_some()
    }

    pub fn install_raw_window_handle<F>(
        &mut self,
        window_handle: RawWindowHandle,
        menu: &UiApplicationMenu,
        mut resolve: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str) -> String,
    {
        let signature = menu_signature(menu, &mut resolve);
        #[cfg(target_os = "windows")]
        {
            let (native_menu, receiver) = build_windows_menu(menu, &mut resolve)?;
            self.replace_windows_menu(window_handle, native_menu, receiver)?;
            self.window_handle = Some(window_handle);
            self.signature = Some(signature);
            self.command_ids = menu.command_ids().into_iter().map(str::to_owned).collect();
            return Ok(());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (window_handle, menu, signature);
            Err("native application menus are not connected for this target yet".to_string())
        }
    }

    pub fn sync_raw_window_handle<F>(
        &mut self,
        window_handle: RawWindowHandle,
        menu: &UiApplicationMenu,
        mut resolve: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str) -> String,
    {
        let signature = menu_signature(menu, &mut resolve);
        if self.window_handle == Some(window_handle) && self.signature.as_ref() == Some(&signature)
        {
            return Ok(());
        }
        self.install_raw_window_handle(window_handle, menu, resolve)
    }

    pub fn sync<F>(&mut self, menu: &UiApplicationMenu, resolve: F) -> Result<(), String>
    where
        F: FnMut(&str) -> String,
    {
        let Some(window_handle) = self.window_handle else {
            return Err("native application menu has no window handle".to_string());
        };
        self.sync_raw_window_handle(window_handle, menu, resolve)
    }

    #[cfg(target_os = "windows")]
    fn replace_windows_menu(
        &mut self,
        window_handle: RawWindowHandle,
        native_menu: WindowsNativeMenu,
        receiver: Receiver<UiMenuActivation>,
    ) -> Result<(), String> {
        let RawWindowHandle::Win32(handle) = window_handle else {
            return Err("the current Windows window has no Win32 handle".to_string());
        };
        let hwnd = handle.hwnd.get();

        if let Some(previous) = self.native_menu.take() {
            previous.detach();
        }
        let native_menu = native_menu.attach(hwnd)?;
        self.native_menu = Some(native_menu);
        self.activations = Some(receiver);
        Ok(())
    }
}

impl NativeApplicationMenuAdapter for NativeWindowApplicationMenuAdapter {
    fn install(&mut self, window: &Window, menu: &UiApplicationMenu) -> Result<(), String> {
        self.install_localized(window, menu, &mut |key| key.to_string())
    }

    fn install_localized(
        &mut self,
        window: &Window,
        menu: &UiApplicationMenu,
        resolve: &mut dyn FnMut(&str) -> String,
    ) -> Result<(), String> {
        let window_handle = window
            .window_handle()
            .map_err(|error| format!("native application menu window handle: {error}"))?
            .as_raw();
        let signature = menu_signature(menu, resolve);
        self.install_raw_window_handle(window_handle, menu, resolve)?;
        self.signature = Some(signature);
        Ok(())
    }

    fn drain_activations(&mut self) -> Vec<UiMenuActivation> {
        self.activations
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default()
    }
}

fn menu_signature(
    menu: &UiApplicationMenu,
    resolve: &mut dyn FnMut(&str) -> String,
) -> MenuSignature {
    let mut labels = Vec::new();
    for section in &menu.menus {
        labels.push(resolve(&section.label_key));
        collect_item_labels(&section.items, resolve, &mut labels);
    }
    MenuSignature {
        menu: menu.clone(),
        labels,
    }
}

fn collect_item_labels(
    items: &[UiMenuItem],
    resolve: &mut dyn FnMut(&str) -> String,
    labels: &mut Vec<String>,
) {
    for item in items {
        match item {
            UiMenuItem::Command(command) => labels.push(resolve(&command.label_key)),
            UiMenuItem::Submenu(menu) => {
                labels.push(resolve(&menu.label_key));
                collect_item_labels(&menu.items, resolve, labels);
            }
            UiMenuItem::Separator => {}
        }
    }
}

#[cfg(target_os = "windows")]
const MFT_OWNERDRAW: u32 = 0x0100;
#[cfg(target_os = "windows")]
const MIIM_STATE: u32 = 0x0001;
#[cfg(target_os = "windows")]
const MIIM_ID: u32 = 0x0002;
#[cfg(target_os = "windows")]
const MIIM_SUBMENU: u32 = 0x0004;
#[cfg(target_os = "windows")]
const MIIM_FTYPE: u32 = 0x0100;
#[cfg(target_os = "windows")]
const MIIM_DATA: u32 = 0x0020;
#[cfg(target_os = "windows")]
const MFS_GRAYED: u32 = 0x0003;
#[cfg(target_os = "windows")]
const MFS_CHECKED: u32 = 0x0008;
#[cfg(target_os = "windows")]
const MIM_BACKGROUND: u32 = 0x0000_0002;
#[cfg(target_os = "windows")]
const MIM_APPLYTOSUBMENUS: u32 = 0x8000_0000;
#[cfg(target_os = "windows")]
const WM_COMMAND: u32 = 0x0111;
#[cfg(target_os = "windows")]
const WM_DRAWITEM: u32 = 0x002B;
#[cfg(target_os = "windows")]
const WM_MEASUREITEM: u32 = 0x002C;
#[cfg(target_os = "windows")]
const ODT_MENU: u32 = 1;
#[cfg(target_os = "windows")]
const WM_NCPAINT: u32 = 0x0085;
#[cfg(target_os = "windows")]
const WM_PAINT: u32 = 0x000F;
#[cfg(target_os = "windows")]
const OBJID_MENU: u32 = 0xFFFF_FFFD;
#[cfg(target_os = "windows")]
const ODS_SELECTED: u32 = 0x0001;
#[cfg(target_os = "windows")]
const ODS_GRAYED: u32 = 0x0002;
#[cfg(target_os = "windows")]
const ODS_DISABLED: u32 = 0x0004;
#[cfg(target_os = "windows")]
const DT_SINGLELINE: u32 = 0x0020;
#[cfg(target_os = "windows")]
const DT_VCENTER: u32 = 0x0004;
#[cfg(target_os = "windows")]
const MENU_BG: u32 = 0x0020_1C18;
#[cfg(target_os = "windows")]
const MENU_HOVER: u32 = 0x003B_332D;
#[cfg(target_os = "windows")]
const MENU_ACCENT: u32 = 0x0022_89E7;
#[cfg(target_os = "windows")]
const MENU_TEXT: u32 = 0x00E5_DCD7;
#[cfg(target_os = "windows")]
const MENU_DISABLED_TEXT: u32 = 0x007A_7672;
#[cfg(target_os = "windows")]
const MENU_SEPARATOR: u32 = 0x0044_3D36;
#[cfg(target_os = "windows")]
const MENU_SUBCLASS_ID: usize = 0x5241_4649;
#[cfg(target_os = "windows")]
const PS_SOLID: u32 = 0;

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsPoint {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsMenuItemInfo {
    cb_size: u32,
    f_mask: u32,
    f_type: u32,
    f_state: u32,
    w_id: u32,
    h_sub_menu: isize,
    hbmp_checked: isize,
    hbmp_unchecked: isize,
    dw_item_data: usize,
    dw_type_data: *mut u16,
    cch: u32,
    hbmp_item: isize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsMenuInfo {
    cb_size: u32,
    f_mask: u32,
    dw_style: u32,
    cy_max: u32,
    hbr_back: isize,
    dw_context_help_id: u32,
    dw_menu_data: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsDrawItemStruct {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: isize,
    hdc: isize,
    rc_item: WindowsRect,
    item_data: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsMeasureItemStruct {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_width: u32,
    item_height: u32,
    item_data: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WindowsMenuBarInfo {
    cb_size: u32,
    rc_bar: WindowsRect,
    h_menu: isize,
    hwnd_menu: isize,
    f_bar_focused: i32,
    f_focused: i32,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn CreateMenu() -> isize;
    fn CreatePopupMenu() -> isize;
    fn InsertMenuItemW(
        menu: isize,
        item: u32,
        by_position: i32,
        item_info: *const WindowsMenuItemInfo,
    ) -> i32;
    fn SetMenuInfo(menu: isize, menu_info: *const WindowsMenuInfo) -> i32;
    fn SetMenu(hwnd: isize, menu: isize) -> i32;
    fn DrawMenuBar(hwnd: isize) -> i32;
    fn GetMenu(hwnd: isize) -> isize;
    fn DestroyMenu(menu: isize) -> i32;
    fn GetMenuItemCount(menu: isize) -> i32;
    fn GetMenuBarInfo(
        hwnd: isize,
        object_id: u32,
        item: u32,
        menu_bar_info: *mut WindowsMenuBarInfo,
    ) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut WindowsRect) -> i32;
    fn GetWindowDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn FillRect(hdc: isize, rect: *const WindowsRect, brush: isize) -> i32;
    fn DrawTextW(
        hdc: isize,
        text: *const u16,
        count: i32,
        rect: *mut WindowsRect,
        format: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> isize;
    fn CreatePen(style: u32, width: i32, color: u32) -> isize;
    fn DeleteObject(object: isize) -> i32;
    fn MoveToEx(hdc: isize, x: i32, y: i32, point: *mut WindowsPoint) -> i32;
    fn LineTo(hdc: isize, x: i32, y: i32) -> i32;
    fn SelectObject(hdc: isize, object: isize) -> isize;
    fn RoundRect(
        hdc: isize,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> i32;
    fn SetTextColor(hdc: isize, color: u32) -> u32;
    fn SetBkMode(hdc: isize, mode: i32) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hwnd: isize,
        callback: Option<
            unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
        >,
        subclass_id: usize,
        reference_data: usize,
    ) -> i32;
    fn RemoveWindowSubclass(
        hwnd: isize,
        callback: Option<
            unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
        >,
        subclass_id: usize,
    ) -> i32;
    fn DefSubclassProc(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
}

#[cfg(target_os = "windows")]
#[link(name = "uxtheme")]
extern "system" {
    fn SetWindowTheme(hwnd: isize, sub_app_name: *const u16, sub_id_list: *const u16) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        attribute: u32,
        value: *const c_void,
        value_size: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> isize;
    fn GetProcAddress(module: isize, procedure_name: *const u8) -> *const c_void;
}

#[cfg(target_os = "windows")]
struct WindowsNativeMenu {
    hwnd: isize,
    root: isize,
    background_brush: isize,
    state: Box<WindowsMenuState>,
}

#[cfg(target_os = "windows")]
struct WindowsMenuState {
    sender: Sender<UiMenuActivation>,
    commands: HashMap<u32, String>,
    owner_draw_items: Vec<Box<WindowsMenuItem>>,
}

#[cfg(target_os = "windows")]
struct WindowsMenuItem {
    label: Vec<u16>,
    separator: bool,
    checked: bool,
    top_level: bool,
}

#[cfg(target_os = "windows")]
impl WindowsNativeMenu {
    fn attach(mut self, hwnd: isize) -> Result<Self, String> {
        apply_windows_dark_mode(hwnd);
        if unsafe {
            SetWindowSubclass(
                hwnd,
                Some(windows_menu_subclass_proc),
                MENU_SUBCLASS_ID,
                (&*self.state) as *const _ as usize,
            )
        } == 0
        {
            return Err("Windows could not attach the native menu event bridge".to_string());
        }
        if unsafe { SetMenu(hwnd, self.root) } == 0 {
            unsafe {
                RemoveWindowSubclass(hwnd, Some(windows_menu_subclass_proc), MENU_SUBCLASS_ID);
            }
            return Err("Windows could not attach the native menu bar".to_string());
        }
        self.hwnd = hwnd;
        unsafe {
            DrawMenuBar(hwnd);
        }
        Ok(self)
    }

    fn detach(self) {
        unsafe {
            RemoveWindowSubclass(
                self.hwnd,
                Some(windows_menu_subclass_proc),
                MENU_SUBCLASS_ID,
            );
            if GetMenu(self.hwnd) == self.root {
                SetMenu(self.hwnd, 0);
                DrawMenuBar(self.hwnd);
            }
            DestroyMenu(self.root);
            if self.background_brush != 0 {
                DeleteObject(self.background_brush);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_windows_dark_mode(hwnd: isize) {
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const ALLOW_DARK_MODE: i32 = 1;

    let dark_mode = ALLOW_DARK_MODE;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_mode as *const i32).cast(),
            size_of::<i32>() as u32,
        );
    }

    let theme: Vec<u16> = "DarkMode_Explorer".encode_utf16().chain([0]).collect();
    let uxtheme_name: Vec<u16> = "uxtheme.dll".encode_utf16().chain([0]).collect();
    unsafe {
        let module = GetModuleHandleW(uxtheme_name.as_ptr());
        if module != 0 {
            if let Some(set_preferred_app_mode) = dynamic_function::<
                unsafe extern "system" fn(i32) -> i32,
            >(module, b"SetPreferredAppMode\0")
            {
                let _ = set_preferred_app_mode(ALLOW_DARK_MODE);
            }
            if let Some(allow_dark_mode_for_window) = dynamic_function::<
                unsafe extern "system" fn(isize, i32) -> i32,
            >(
                module, b"AllowDarkModeForWindow\0"
            ) {
                let _ = allow_dark_mode_for_window(hwnd, ALLOW_DARK_MODE);
            }
            if let Some(flush_menu_themes) =
                dynamic_function::<unsafe extern "system" fn()>(module, b"FlushMenuThemes\0")
            {
                flush_menu_themes();
            }
        }
        let _ = SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
    }
}

#[cfg(target_os = "windows")]
unsafe fn dynamic_function<T>(module: isize, procedure_name: &[u8]) -> Option<T> {
    let procedure = GetProcAddress(module, procedure_name.as_ptr());
    if procedure.is_null() {
        None
    } else {
        Some(std::mem::transmute_copy(&procedure))
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn windows_menu_subclass_proc(
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    _subclass_id: usize,
    reference_data: usize,
) -> isize {
    if message == WM_COMMAND {
        let command_id = (wparam & 0xffff) as u32;
        let state = &*(reference_data as *const WindowsMenuState);
        if let Some(command_id) = state.commands.get(&command_id) {
            let _ = state.sender.send(UiMenuActivation::new(command_id.clone()));
            return 0;
        }
    }
    if message == WM_MEASUREITEM && lparam != 0 {
        let measure = &mut *(lparam as *mut WindowsMeasureItemStruct);
        if measure.ctl_type == ODT_MENU && measure.item_data != 0 {
            let item = &*(measure.item_data as *const WindowsMenuItem);
            let character_count = item.label.len().saturating_sub(1) as u32;
            measure.item_width = if item.separator {
                1
            } else if item.top_level {
                20 + character_count.saturating_mul(8)
            } else {
                48 + character_count.saturating_mul(8)
            };
            measure.item_height = if item.separator {
                10
            } else if item.top_level {
                34
            } else {
                36
            };
            return 1;
        }
    }
    if message == WM_DRAWITEM && lparam != 0 {
        let draw = &*(lparam as *const WindowsDrawItemStruct);
        if draw.ctl_type == ODT_MENU && draw.item_data != 0 {
            draw_windows_menu_item(draw);
            return 1;
        }
    }
    let result = DefSubclassProc(hwnd, message, wparam, lparam);
    if message == WM_PAINT || message == WM_NCPAINT {
        paint_windows_menu_edge(hwnd);
    }
    result
}

#[cfg(target_os = "windows")]
unsafe fn paint_windows_menu_edge(hwnd: isize) {
    let mut menu_bar = WindowsMenuBarInfo {
        cb_size: std::mem::size_of::<WindowsMenuBarInfo>() as u32,
        rc_bar: WindowsRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        h_menu: 0,
        hwnd_menu: 0,
        f_bar_focused: 0,
        f_focused: 0,
    };
    if GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut menu_bar) == 0 {
        return;
    }

    let mut window_rect = WindowsRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetWindowRect(hwnd, &mut window_rect) == 0 {
        return;
    }
    let menu_bottom = menu_bar.rc_bar.bottom - window_rect.top;
    let edge_rect = WindowsRect {
        left: menu_bar.rc_bar.left - window_rect.left,
        top: menu_bottom - 2,
        right: menu_bar.rc_bar.right - window_rect.left,
        bottom: menu_bottom,
    };
    let hdc = GetWindowDC(hwnd);
    if hdc == 0 {
        return;
    }
    let brush = CreateSolidBrush(MENU_BG);
    if brush != 0 {
        FillRect(hdc, &edge_rect, brush);
        DeleteObject(brush);
    }
    ReleaseDC(hwnd, hdc);
}

#[cfg(target_os = "windows")]
unsafe fn draw_windows_menu_item(draw: &WindowsDrawItemStruct) {
    let item = &*(draw.item_data as *const WindowsMenuItem);
    let selected = draw.item_state & ODS_SELECTED != 0;
    let disabled = draw.item_state & (ODS_GRAYED | ODS_DISABLED) != 0;
    let text_color = if disabled {
        MENU_DISABLED_TEXT
    } else {
        MENU_TEXT
    };

    let background_brush = CreateSolidBrush(MENU_BG);
    if background_brush != 0 {
        FillRect(draw.hdc, &draw.rc_item, background_brush);
        DeleteObject(background_brush);
    }

    if selected {
        let selection_rect = WindowsRect {
            left: draw.rc_item.left + 3,
            top: draw.rc_item.top + 2,
            right: draw.rc_item.right - 3,
            bottom: draw.rc_item.bottom - 2,
        };
        draw_windows_rounded_fill(draw.hdc, &selection_rect, MENU_HOVER);

        let accent_rect = if item.top_level {
            WindowsRect {
                left: draw.rc_item.left + 9,
                top: draw.rc_item.bottom - 3,
                right: draw.rc_item.right - 9,
                bottom: draw.rc_item.bottom - 1,
            }
        } else {
            WindowsRect {
                left: draw.rc_item.left + 7,
                top: draw.rc_item.top + 9,
                right: draw.rc_item.left + 9,
                bottom: draw.rc_item.bottom - 9,
            }
        };
        let accent_brush = CreateSolidBrush(MENU_ACCENT);
        if accent_brush != 0 {
            FillRect(draw.hdc, &accent_rect, accent_brush);
            DeleteObject(accent_brush);
        }
    }

    if item.separator {
        let middle = (draw.rc_item.top + draw.rc_item.bottom) / 2;
        let separator_rect = WindowsRect {
            left: draw.rc_item.left + 8,
            top: middle,
            right: draw.rc_item.right - 8,
            bottom: middle + 1,
        };
        let separator_brush = CreateSolidBrush(MENU_SEPARATOR);
        if separator_brush != 0 {
            FillRect(draw.hdc, &separator_rect, separator_brush);
            DeleteObject(separator_brush);
        }
        return;
    }

    if item.checked {
        let check_pen = CreatePen(PS_SOLID, 2, text_color);
        if check_pen != 0 {
            let previous_pen = SelectObject(draw.hdc, check_pen);
            MoveToEx(
                draw.hdc,
                draw.rc_item.left + 13,
                draw.rc_item.top + 18,
                std::ptr::null_mut(),
            );
            LineTo(draw.hdc, draw.rc_item.left + 17, draw.rc_item.top + 22);
            LineTo(draw.hdc, draw.rc_item.left + 25, draw.rc_item.top + 12);
            SelectObject(draw.hdc, previous_pen);
            DeleteObject(check_pen);
        }
    }

    let _ = SetBkMode(draw.hdc, 1);
    let _ = SetTextColor(draw.hdc, text_color);
    let mut text_rect = WindowsRect {
        left: draw.rc_item.left + if item.top_level { 12 } else { 28 },
        top: draw.rc_item.top,
        right: draw.rc_item.right - 12,
        bottom: draw.rc_item.bottom,
    };
    let text_length = item.label.len().saturating_sub(1) as i32;
    DrawTextW(
        draw.hdc,
        item.label.as_ptr(),
        text_length,
        &mut text_rect,
        DT_SINGLELINE | DT_VCENTER,
    );
}

#[cfg(target_os = "windows")]
unsafe fn draw_windows_rounded_fill(hdc: isize, rect: &WindowsRect, color: u32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, color);
    if brush == 0 || pen == 0 {
        if brush != 0 {
            DeleteObject(brush);
        }
        if pen != 0 {
            DeleteObject(pen);
        }
        return;
    }
    let previous_brush = SelectObject(hdc, brush);
    let previous_pen = SelectObject(hdc, pen);
    RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 8, 8);
    SelectObject(hdc, previous_brush);
    SelectObject(hdc, previous_pen);
    DeleteObject(brush);
    DeleteObject(pen);
}

#[cfg(target_os = "windows")]
fn build_windows_menu(
    menu: &UiApplicationMenu,
    resolve: &mut dyn FnMut(&str) -> String,
) -> Result<(WindowsNativeMenu, Receiver<UiMenuActivation>), String> {
    let root = unsafe { CreateMenu() };
    if root == 0 {
        return Err("Windows could not create the native menu".to_string());
    }
    let (sender, receiver) = mpsc::channel();
    let mut state = Box::new(WindowsMenuState {
        sender,
        commands: HashMap::new(),
        owner_draw_items: Vec::new(),
    });
    let result = (|| {
        for section in &menu.menus {
            let submenu = unsafe { CreatePopupMenu() };
            if submenu == 0 {
                return Err("Windows could not create a native menu section".to_string());
            }
            append_windows_items(submenu, &section.items, resolve, &mut state)?;
            append_windows_owner_draw_item(
                root,
                &resolve(&section.label_key),
                submenu,
                None,
                true,
                false,
                false,
                true,
                &mut state,
            )?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        unsafe {
            DestroyMenu(root);
        }
        return Err(error);
    }
    let background_brush = unsafe { CreateSolidBrush(MENU_BG) };
    if background_brush != 0 {
        let menu_info = WindowsMenuInfo {
            cb_size: std::mem::size_of::<WindowsMenuInfo>() as u32,
            f_mask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS,
            dw_style: 0,
            cy_max: 0,
            hbr_back: background_brush,
            dw_context_help_id: 0,
            dw_menu_data: 0,
        };
        unsafe {
            SetMenuInfo(root, &menu_info);
        }
    }
    Ok((
        WindowsNativeMenu {
            hwnd: 0,
            root,
            background_brush,
            state,
        },
        receiver,
    ))
}

#[cfg(target_os = "windows")]
fn append_windows_items(
    parent: isize,
    items: &[UiMenuItem],
    resolve: &mut dyn FnMut(&str) -> String,
    state: &mut WindowsMenuState,
) -> Result<(), String> {
    for item in items {
        match item {
            UiMenuItem::Separator => append_windows_owner_draw_item(
                parent, "", 0, None, true, false, true, false, state,
            )?,
            UiMenuItem::Command(command) => {
                let id = next_windows_command_id();
                state.commands.insert(id, command.id.clone());
                append_windows_owner_draw_item(
                    parent,
                    &resolve(&command.label_key),
                    0,
                    Some(id),
                    command.enabled,
                    command.checked,
                    false,
                    false,
                    state,
                )?;
            }
            UiMenuItem::Submenu(menu) => {
                let submenu = unsafe { CreatePopupMenu() };
                if submenu == 0 {
                    return Err("Windows could not create a native submenu".to_string());
                }
                append_windows_items(submenu, &menu.items, resolve, state)?;
                append_windows_owner_draw_item(
                    parent,
                    &resolve(&menu.label_key),
                    submenu,
                    None,
                    true,
                    false,
                    false,
                    false,
                    state,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn append_windows_owner_draw_item(
    parent: isize,
    label: &str,
    submenu: isize,
    command_id: Option<u32>,
    enabled: bool,
    checked: bool,
    separator: bool,
    top_level: bool,
    state: &mut WindowsMenuState,
) -> Result<(), String> {
    let mut text: Vec<u16> = label.encode_utf16().collect();
    text.push(0);
    let item = Box::new(WindowsMenuItem {
        label: text,
        separator,
        checked,
        top_level,
    });
    let item_data = (&*item) as *const WindowsMenuItem as usize;
    state.owner_draw_items.push(item);

    let position = unsafe { GetMenuItemCount(parent) };
    if position < 0 {
        return Err("Windows could not read the native menu position".to_string());
    }
    let mut item_info = WindowsMenuItemInfo {
        cb_size: std::mem::size_of::<WindowsMenuItemInfo>() as u32,
        f_mask: MIIM_FTYPE | MIIM_DATA,
        f_type: MFT_OWNERDRAW,
        f_state: 0,
        w_id: 0,
        h_sub_menu: 0,
        hbmp_checked: 0,
        hbmp_unchecked: 0,
        dw_item_data: item_data,
        dw_type_data: std::ptr::null_mut(),
        cch: 0,
        hbmp_item: 0,
    };
    if submenu != 0 {
        item_info.f_mask |= MIIM_SUBMENU;
        item_info.h_sub_menu = submenu;
    }
    if let Some(command_id) = command_id {
        item_info.f_mask |= MIIM_ID;
        item_info.w_id = command_id;
        if !enabled {
            item_info.f_mask |= MIIM_STATE;
            item_info.f_state |= MFS_GRAYED;
        }
        if checked {
            item_info.f_mask |= MIIM_STATE;
            item_info.f_state |= MFS_CHECKED;
        }
    }
    if unsafe { InsertMenuItemW(parent, position as u32, 1, &item_info) } == 0 {
        return Err("Windows could not append a native menu item".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn next_windows_command_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT_ID: AtomicU32 = AtomicU32::new(0x5000);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ui::{UiMenu, UiMenuCommand, UiMenuItem};

    #[derive(Default)]
    struct RecordingAdapter {
        installed_command_ids: Vec<String>,
        activations: Vec<UiMenuActivation>,
    }

    impl NativeApplicationMenuAdapter for RecordingAdapter {
        fn install(&mut self, _window: &Window, menu: &UiApplicationMenu) -> Result<(), String> {
            self.installed_command_ids =
                menu.command_ids().into_iter().map(str::to_string).collect();
            Ok(())
        }

        fn drain_activations(&mut self) -> Vec<UiMenuActivation> {
            std::mem::take(&mut self.activations)
        }
    }

    #[test]
    fn command_model_is_platform_independent() {
        let menu = UiApplicationMenu {
            menus: vec![
                UiMenu::new("file", "app.file").with_item(UiMenuItem::Command(UiMenuCommand::new(
                    "project.save",
                    "app.save_menu",
                ))),
            ],
        };
        let mut adapter = RecordingAdapter::default();

        adapter.installed_command_ids =
            menu.command_ids().into_iter().map(str::to_string).collect();
        adapter
            .activations
            .push(UiMenuActivation::new("project.save"));

        assert_eq!(adapter.installed_command_ids, ["project.save"]);
        assert_eq!(adapter.drain_activations()[0].command_id, "project.save");
    }
}
