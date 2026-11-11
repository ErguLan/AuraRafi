//! Bounded clipboard ingestion outside the UI thread.
use raf_ai::chat_media::{ChatImage, MAX_IMAGE_BYTES};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
};

pub(crate) type MediaReceiver = Receiver<Vec<Result<ChatImage, String>>>;

pub(crate) fn clipboard_job(root: PathBuf) -> MediaReceiver {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(vec![save_clipboard(root)]);
    });
    rx
}

pub(crate) fn files_job(root: PathBuf, files: Vec<PathBuf>) -> MediaReceiver {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let results = files
            .iter()
            .take(raf_ai::chat_media::MAX_IMAGES)
            .map(|path| import_file(&root, path))
            .collect();
        let _ = tx.send(results);
    });
    rx
}

fn import_file(root: &Path, source: &Path) -> Result<ChatImage, String> {
    use std::io::Read;
    // User-authorized local file reads only; normalized copies are project-owned.
    let mut bytes = Vec::new();
    let file = std::fs::File::open(source).map_err(|_| "agent.media.invalid_image")?;
    if !file
        .metadata()
        .map_err(|_| "agent.media.invalid_image")?
        .is_file()
    {
        return Err("agent.media.invalid_image".into());
    }
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "agent.media.invalid_image")?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("agent.media.too_large".into());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "agent.media.invalid_image")?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("agent.media.invalid_image".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| "agent.media.invalid_image")?;
    save_image(root, image)
}

fn save_clipboard(root: PathBuf) -> Result<ChatImage, String> {
    use raf_render::api_graphic_basic::ui_surface::NativeUiInputBridge;
    let (width, height, rgba) =
        NativeUiInputBridge::read_clipboard_rgba().ok_or("agent.media.no_image")?;
    let source =
        image::RgbaImage::from_raw(width, height, rgba).ok_or("agent.media.invalid_image")?;
    save_image(&root, image::DynamicImage::ImageRgba8(source))
}

fn save_image(root: &Path, source: image::DynamicImage) -> Result<ChatImage, String> {
    let normalized = source.thumbnail(1024, 1024);
    let mut buffer = std::io::Cursor::new(Vec::new());
    normalized
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|_| "agent.media.invalid_image")?;
    if buffer.get_ref().len() as u64 > MAX_IMAGE_BYTES {
        return Err("agent.media.too_large".into());
    }
    let project = root.canonicalize().map_err(|_| "agent.media.save_failed")?;
    let ai = project.join(".ai");
    if ai.exists()
        && !ai
            .canonicalize()
            .map_err(|_| "agent.media.save_failed")?
            .starts_with(&project)
    {
        return Err("agent.media.save_failed".into());
    }
    std::fs::create_dir_all(&ai).map_err(|_| "agent.media.save_failed")?;
    let media = ai.join("agent_media");
    if media.exists()
        && !media
            .canonicalize()
            .map_err(|_| "agent.media.save_failed")?
            .starts_with(&project)
    {
        return Err("agent.media.save_failed".into());
    }
    std::fs::create_dir_all(&media).map_err(|_| "agent.media.save_failed")?;
    let mut total = 0u64;
    let mut count = 0;
    for entry in std::fs::read_dir(&media)
        .map_err(|_| "agent.media.save_failed")?
        .take(257)
    {
        total = total.saturating_add(
            entry
                .map_err(|_| "agent.media.save_failed")?
                .metadata()
                .map_err(|_| "agent.media.save_failed")?
                .len(),
        );
        count += 1;
    }
    if count >= 256 || total + buffer.get_ref().len() as u64 > 64 * 1024 * 1024 {
        return Err("agent.media.storage_full".into());
    }
    let file = format!("{}.png", uuid::Uuid::new_v4());
    use std::io::Write;
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(media.join(&file))
        .map_err(|_| "agent.media.save_failed")?;
    destination
        .write_all(buffer.get_ref())
        .and_then(|_| destination.sync_all())
        .map_err(|_| "agent.media.save_failed")?;
    Ok(ChatImage {
        file,
        width: normalized.width(),
        height: normalized.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_image_import_normalizes_and_builds_real_multimodal_content() {
        let directory =
            std::env::temp_dir().join(format!("raf-agent-media-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("reference.png");
        image::DynamicImage::new_rgba8(2048, 512)
            .save(&source)
            .unwrap();
        let attachment = import_file(&directory, &source).unwrap();
        assert_eq!((attachment.width, attachment.height), (1024, 256));
        assert!(source.exists(), "import must not move the original");
        let stored = directory.join(".ai/agent_media").join(&attachment.file);
        let decoded = image::open(&stored).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1024, 256));
        let part = raf_ai::chat_media::image_part(&directory, &attachment).unwrap();
        assert_eq!(part["type"], "image_url");
        assert!(part["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,iVBOR"));
        assert!(import_file(&directory, &directory).is_err());
        assert!(raf_ai::chat_media::image_part(
            &directory,
            &ChatImage {
                file: "../reference.png".into(),
                width: 1,
                height: 1
            }
        )
        .is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
