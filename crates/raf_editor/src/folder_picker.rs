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

    pub(super) fn pick_asset_file() -> Option<PathBuf> {
        unsafe {
            show_open_dialog(
                "Import asset",
                false,
                "Asset files (*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai)",
                "*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai",
            )
        }
    }
}

/// Normalizes the first non-empty line produced by a platform dialog helper.
/// External helpers are allowed to append warnings or trailing newlines.
fn normalize_picker_path(output: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(output);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(PathBuf::from(line))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;
    use std::process::Command;

    use super::normalize_picker_path;

    fn run(command: &mut Command) -> Option<PathBuf> {
        let output = command.output().ok()?;
        output
            .status
            .success()
            .then(|| normalize_picker_path(&output.stdout))
            .flatten()
    }

    pub(super) fn pick_directory(title: &str) -> Option<PathBuf> {
        let title_arg = format!("--title={title}");
        if let Some(path) = run(Command::new("zenity").args([
            "--file-selection",
            "--directory",
            title_arg.as_str(),
        ])) {
            return Some(path);
        }
        run(Command::new("kdialog").args(["--getexistingdirectory", ".", "--title", title]))
    }

    pub(super) fn pick_file(title: &str, filter_name: &str, pattern: &str) -> Option<PathBuf> {
        let title_arg = format!("--title={title}");
        let filter = format!("--file-filter={filter_name} | {pattern}");
        if let Some(path) = run(Command::new("zenity").args([
            "--file-selection",
            title_arg.as_str(),
            filter.as_str(),
        ])) {
            return Some(path);
        }
        run(Command::new("kdialog").args(["--getopenfilename", ".", pattern, "--title", title]))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;
    use std::process::Command;

    use super::normalize_picker_path;

    fn escape(script: &str) -> String {
        script.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn run(script: &str) -> Option<PathBuf> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| normalize_picker_path(&output.stdout))
            .flatten()
    }

    pub(super) fn pick_directory(title: &str) -> Option<PathBuf> {
        run(&format!(
            "POSIX path of (choose folder with prompt \"{}\")",
            escape(title)
        ))
    }

    pub(super) fn pick_file(title: &str, _filter_name: &str, pattern: &str) -> Option<PathBuf> {
        let extension = PathBuf::from(pattern)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("*");
        run(&format!(
            "POSIX path of (choose file with prompt \"{}\" of type {{\"{}\"}})",
            escape(title),
            escape(extension)
        ))
    }
}

/// Opens the platform folder picker used by the create form. The current
/// location remains the form's fallback when the user cancels the dialog.
pub fn pick_folder(_current_location: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::browse_location()
    }

    #[cfg(target_os = "linux")]
    {
        linux::pick_directory("Choose project location")
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_directory("Choose project location")
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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

    #[cfg(target_os = "linux")]
    {
        linux::pick_directory("Open AuraRafi project folder")
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_directory("Open AuraRafi project folder")
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Opens the browser-style picker used by Assets > Import.
pub fn pick_asset_file() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::pick_asset_file()
    }

    #[cfg(target_os = "linux")]
    {
        linux::pick_file(
            "Import asset",
            "Asset files (*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai)",
            "*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai",
        )
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_file(
            "Import asset",
            "Asset files (*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai)",
            "*.glb;*.gltf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.ogg;*.rhai",
        )
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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

    #[cfg(target_os = "linux")]
    {
        linux::pick_file(
            "Open AuraRafi project file",
            "AuraRafi project (*.ron)",
            "*.ron",
        )
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_file(
            "Open AuraRafi project file",
            "AuraRafi project (*.ron)",
            "*.ron",
        )
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Destination selection owned by the running editor, without opening another application.
pub fn pick_output_folder(title: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::pick_output_folder(title)
    }

    #[cfg(target_os = "linux")]
    {
        linux::pick_directory(title)
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_directory(title)
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = title;
        None
    }
}

pub(crate) fn pick_firmware(title: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        win32::pick_firmware(title)
    }

    #[cfg(target_os = "linux")]
    {
        linux::pick_file(title, "Firmware (*.bin)", "*.bin")
    }

    #[cfg(target_os = "macos")]
    {
        macos::pick_file(title, "Firmware (*.bin)", "*.bin")
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = title;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_picker_path;

    #[test]
    fn platform_picker_output_is_trimmed_to_the_first_path() {
        let path =
            normalize_picker_path(b"\r\n  /tmp/firmware.bin  \r\nwarning: ignored\n").unwrap();
        assert_eq!(path, std::path::PathBuf::from("/tmp/firmware.bin"));
    }

    #[test]
    fn an_empty_or_cancelled_picker_returns_no_path() {
        assert!(normalize_picker_path(b"").is_none());
        assert!(normalize_picker_path(b" \n\r\n").is_none());
    }
}
