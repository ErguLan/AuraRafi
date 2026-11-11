//! Native folder and project-file selection kept at the editor/platform
//! boundary.

use std::path::PathBuf;

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::path::PathBuf;

    use windows::core::{IUnknown, Interface, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileDialog, IFileOpenDialog, IShellItem, FOS_FILEMUSTEXIST,
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };

    #[repr(C)]
    struct ItemIdList {
        _opaque: [u8; 1],
    }

    #[repr(C)]
    struct BrowseInfoW {
        hwnd_owner: *mut c_void,
        pidl_root: *mut ItemIdList,
        display_name: *mut u16,
        title: *const u16,
        flags: u32,
        callback: Option<unsafe extern "system" fn(*mut c_void, u32, isize, isize) -> i32>,
        lparam: isize,
        image: i32,
    }

    const BIF_RETURNONLYFSDIRS: u32 = 0x0001;
    const BIF_NEWDIALOGSTYLE: u32 = 0x0040;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHBrowseForFolderW(info: *const BrowseInfoW) -> *mut ItemIdList;
        fn SHGetPathFromIDListW(id_list: *const ItemIdList, path: *mut u16) -> i32;
    }

    /// Balances `CoInitializeEx` without ever leaving the UI thread in a
    /// different apartment than the one the application already owns.
    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn browse_for_folder() -> Option<PathBuf> {
        let title: Vec<u16> = "Choose project location\0".encode_utf16().collect();
        let mut display_name = [0u16; 260];
        let info = BrowseInfoW {
            hwnd_owner: std::ptr::null_mut(),
            pidl_root: std::ptr::null_mut(),
            display_name: display_name.as_mut_ptr(),
            title: title.as_ptr(),
            flags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            callback: None,
            lparam: 0,
            image: 0,
        };

        let id_list = SHBrowseForFolderW(&info);
        if id_list.is_null() {
            return None;
        }

        let mut path = [0u16; 32_768];
        let result = SHGetPathFromIDListW(id_list, path.as_mut_ptr());
        CoTaskMemFree(Some(id_list as *const c_void));
        if result == 0 {
            return None;
        }

        let length = path
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(path.len());
        Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
    }

    /// Browser-style dialog. `folder` switches between selecting a directory
    /// and selecting a `.ron` project file.
    unsafe fn show_open_dialog(
        title: &str,
        folder: bool,
        filter_name: &str,
        filter_spec: &str,
    ) -> Option<PathBuf> {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let _guard = ComGuard(initialized);

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        let file_dialog: IFileDialog = dialog.cast().ok()?;
        let title = wide(title);
        file_dialog.SetTitle(PCWSTR(title.as_ptr())).ok()?;

        let mut options = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST;
        if folder {
            options |= FOS_PICKFOLDERS;
        } else {
            options |= FOS_FILEMUSTEXIST;
        }
        file_dialog.SetOptions(options).ok()?;

        // The filter strings must outlive the dialog call.
        let _filter_storage = if folder {
            None
        } else {
            let name = wide(filter_name);
            let spec = wide(filter_spec);
            let filters = [COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: PCWSTR(spec.as_ptr()),
            }];
            file_dialog.SetFileTypes(&filters).ok()?;
            Some((name, spec))
        };

        // A cancelled dialog returns an error; callers treat it as "no choice".
        dialog.Show(HWND::default()).ok()?;
        let item: IShellItem = dialog.GetResult().ok()?;
        let display = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = display.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(display.0 as *const c_void));
        path
    }

    pub(super) fn pick_output_folder(title: &str) -> Option<PathBuf> {
        unsafe { show_open_dialog(title, true, "", "") }
    }

    pub(super) fn pick_firmware(title: &str) -> Option<PathBuf> {
        unsafe { show_open_dialog(title, false, "Firmware (*.bin)", "*.bin") }
    }

    pub(super) fn browse_location() -> Option<PathBuf> {
        unsafe { browse_for_folder() }
    }

    pub(super) fn pick_project_folder() -> Option<PathBuf> {
        unsafe {
            show_open_dialog(
                "Open AuraRafi project folder",
                true,
                "AuraRafi project",
                "*.ron",
            )
        }
    }

    pub(super) fn pick_project_file() -> Option<PathBuf> {
        unsafe {
            show_open_dialog(
                "Open AuraRafi project file",
                false,
                "AuraRafi project (*.ron)",
                "*.ron",
            )
        }
    }
}

/// Opens the platform folder picker used by the create form. The current
/// location remains the form's fallback when the user cancels the dialog.
pub fn pick_folder(_current_location: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::browse_location()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Opens the browser-style picker for an existing project folder.
pub fn pick_project_folder(_current_location: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::pick_project_folder()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Opens the browser-style picker filtered to `.ron` project files.
pub fn pick_project_file(_current_location: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::pick_project_file()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

/// Destination selection owned by the running editor, without opening another application.
pub fn pick_output_folder(title: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    { win32::pick_output_folder(title) }
    #[cfg(not(windows))]
    { let _ = title; None }
}

pub(crate) fn pick_firmware(title: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    { win32::pick_firmware(title) }
    #[cfg(not(windows))]
    { let _ = title; None }
}
