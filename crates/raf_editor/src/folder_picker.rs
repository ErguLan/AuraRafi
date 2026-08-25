//! Native folder selection kept at the editor/platform boundary.

use std::path::PathBuf;

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::path::PathBuf;

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

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(pointer: *mut c_void);
    }

    pub(super) fn pick_folder() -> Option<PathBuf> {
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

        let id_list = unsafe { SHBrowseForFolderW(&info) };
        if id_list.is_null() {
            return None;
        }

        let mut path = [0u16; 32_768];
        let result = unsafe { SHGetPathFromIDListW(id_list, path.as_mut_ptr()) };
        unsafe { CoTaskMemFree(id_list.cast()) };
        if result == 0 {
            return None;
        }

        let length = path
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(path.len());
        Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
    }
}

/// Opens the platform folder picker. The current location remains the form's
/// fallback when the user cancels the dialog.
pub fn pick_folder(_current_location: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        windows::pick_folder()
    }

    #[cfg(not(windows))]
    {
        None
    }
}
