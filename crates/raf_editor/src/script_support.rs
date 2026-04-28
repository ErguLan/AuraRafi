use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLanguage {
    Rust,
    Cpp,
    Rhai,
    Lua,
    Python,
    JavaScript,
    TypeScript,
    Unknown,
}

impl ScriptLanguage {
    pub fn from_path(path: &str) -> Self {
        let lower = path.to_lowercase();
        if lower.ends_with(".rs") {
            Self::Rust
        } else if lower.ends_with(".cpp") || lower.ends_with(".cc") || lower.ends_with(".cxx") {
            Self::Cpp
        } else if lower.ends_with(".rhai") {
            Self::Rhai
        } else if lower.ends_with(".lua") {
            Self::Lua
        } else if lower.ends_with(".py") {
            Self::Python
        } else if lower.ends_with(".js") {
            Self::JavaScript
        } else if lower.ends_with(".ts") {
            Self::TypeScript
        } else {
            Self::Unknown
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Cpp => "C++",
            Self::Rhai => "Rhai",
            Self::Lua => "Lua",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_engine_supported(self) -> bool {
        matches!(self, Self::Rust | Self::Cpp | Self::Rhai)
    }
}

#[derive(Debug, Clone)]
pub struct ScriptCatalogEntry {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub language: ScriptLanguage,
    pub has_on_start: bool,
    pub has_on_update: bool,
}

#[derive(Debug, Clone)]
pub struct ScriptValidation {
    pub exists: bool,
    pub supported: bool,
    pub language: ScriptLanguage,
    pub has_on_start: bool,
    pub has_on_update: bool,
    pub absolute_path: Option<PathBuf>,
}

pub fn is_script_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".lua")
        || lower.ends_with(".rhai")
        || lower.ends_with(".py")
        || lower.ends_with(".rs")
        || lower.ends_with(".cpp")
        || lower.ends_with(".cc")
        || lower.ends_with(".cxx")
        || lower.ends_with(".js")
        || lower.ends_with(".ts")
}

pub fn asset_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn scan_script_catalog(assets_root: &Path) -> Vec<ScriptCatalogEntry> {
    if !assets_root.exists() {
        return Vec::new();
    }

    let mut stack = vec![assets_root.to_path_buf()];
    let mut scripts = Vec::new();

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !is_script_file(file_name) {
                continue;
            }

            let (has_on_start, has_on_update) = analyze_script_file(&path);
            scripts.push(ScriptCatalogEntry {
                relative_path: asset_relative_path(assets_root, &path),
                absolute_path: path.clone(),
                language: ScriptLanguage::from_path(file_name),
                has_on_start,
                has_on_update,
            });
        }
    }

    scripts.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    scripts
}

pub fn validate_attached_script(assets_root: Option<&Path>, relative_path: &str) -> ScriptValidation {
    let language = ScriptLanguage::from_path(relative_path);
    let supported = language.is_engine_supported();

    let Some(root) = assets_root else {
        return ScriptValidation {
            exists: false,
            supported,
            language,
            has_on_start: false,
            has_on_update: false,
            absolute_path: None,
        };
    };

    let absolute_path = root.join(PathBuf::from(relative_path));
    if !absolute_path.exists() {
        return ScriptValidation {
            exists: false,
            supported,
            language,
            has_on_start: false,
            has_on_update: false,
            absolute_path: Some(absolute_path),
        };
    }

    let (has_on_start, has_on_update) = analyze_script_file(&absolute_path);
    ScriptValidation {
        exists: true,
        supported,
        language,
        has_on_start,
        has_on_update,
        absolute_path: Some(absolute_path),
    }
}

pub fn script_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

pub fn open_script_in_external_editor(path: &Path) -> bool {
    if std::process::Command::new("code")
        .arg(path.as_os_str())
        .spawn()
        .is_ok()
    {
        return true;
    }

    #[cfg(target_os = "windows")]
    {
        if std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if std::process::Command::new("open")
            .arg(path.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("xdg-open")
            .arg(path.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    false
}

pub fn open_path_in_file_manager(path: &Path) -> bool {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(path).to_path_buf()
    };

    #[cfg(target_os = "windows")]
    {
        if std::process::Command::new("explorer")
            .arg(target.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if std::process::Command::new("open")
            .arg(target.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("xdg-open")
            .arg(target.as_os_str())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }

    false
}

fn analyze_script_file(path: &Path) -> (bool, bool) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return (false, false);
    };

    let lower = contents.to_lowercase();
    let has_on_start = lower.contains("fn on_start") || lower.contains("void on_start(");
    let has_on_update = lower.contains("fn on_update") || lower.contains("void on_update(");
    (has_on_start, has_on_update)
}