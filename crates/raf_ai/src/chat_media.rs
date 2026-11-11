//! Project-confined image references; PNG bytes never live in chat history.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Component, Path};

pub const MAX_IMAGES: usize = 4;
pub const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatImage {
    pub file: String,
    pub width: u32,
    pub height: u32,
}

/// A text-only follow-up must not implicitly resend older private images.
pub(crate) fn latest_user_images(
    messages: &[crate::chat::ChatMessage],
) -> Option<(usize, Vec<ChatImage>)> {
    messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| m.role == crate::chat::MessageRole::User)
        .filter(|(_, m)| !m.images.is_empty())
        .map(|(i, m)| (i, m.images.clone()))
}

/// Called on the provider worker, not during UI layout/input.
pub fn image_part(root: &Path, image: &ChatImage) -> Result<Value, String> {
    let file = resolve_project_image(root, &image.file)?;
    let project = root.canonicalize().map_err(|e| e.to_string())?;
    if !file.starts_with(&project) {
        return Err("Image reference escapes the active project.".into());
    }
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err("Image exceeds the attachment budget.".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|e| e.to_string())?
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Image is missing, oversized or not a PNG.".into());
    }
    Ok(
        json!({"type":"image_url", "image_url":{"url":format!("data:image/png;base64,{}", base64(&bytes))}}),
    )
}

fn resolve_project_image(root: &Path, file: &str) -> Result<std::path::PathBuf, String> {
    let relative = Path::new(file);
    if relative.extension().and_then(|s| s.to_str()) != Some("png")
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Invalid project image reference.".into());
    }
    let components = relative
        .components()
        .filter_map(|part| match part {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let directory = match components.as_slice() {
        [_] => ".ai/agent_media".to_string(),
        [".aura_rafi", "agent_artifacts", _] => ".aura_rafi/agent_artifacts".to_string(),
        [".ai", "agent_media", _] => ".ai/agent_media".to_string(),
        _ => return Err("Invalid project image reference.".into()),
    };
    let project = root.canonicalize().map_err(|e| e.to_string())?;
    let directory = project
        .join(directory)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !directory.starts_with(&project) {
        return Err("Image reference escapes the active project.".into());
    }
    let resolved = directory
        .join(components.last().copied().unwrap_or_default())
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !resolved.starts_with(&directory) {
        return Err("Image reference escapes the active project.".into());
    }
    Ok(resolved)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for part in bytes.chunks(3) {
        let bits = ((part[0] as u32) << 16)
            | ((part.get(1).copied().unwrap_or(0) as u32) << 8)
            | part.get(2).copied().unwrap_or(0) as u32;
        output.push(TABLE[((bits >> 18) & 63) as usize] as char);
        output.push(TABLE[((bits >> 12) & 63) as usize] as char);
        output.push(if part.len() > 1 {
            TABLE[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if part.len() > 2 {
            TABLE[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn harness_image_encoding_and_path_guards() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        for file in [
            "../secret.png",
            "C:/secret.png",
            "foo/bar.png",
            "secret.txt",
        ] {
            assert!(image_part(
                Path::new("unused"),
                &ChatImage {
                    file: file.into(),
                    width: 1,
                    height: 1
                }
            )
            .is_err());
        }
    }

    #[test]
    fn evidence_artifact_paths_are_project_confined() {
        let root = std::env::temp_dir().join(format!(
            "raf-chat-media-{}",
            uuid::Uuid::new_v4()
        ));
        let artifacts = root.join(".aura_rafi/agent_artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let png = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78,
            0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
            0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(artifacts.join("front.png"), png).unwrap();
        let image = ChatImage {
            file: ".aura_rafi/agent_artifacts/front.png".into(),
            width: 1,
            height: 1,
        };
        assert!(image_part(&root, &image).is_ok());
        let escape = ChatImage {
            file: ".aura_rafi/agent_artifacts/../escape.png".into(),
            width: 1,
            height: 1,
        };
        assert!(image_part(&root, &escape).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn harness_text_followups_do_not_resend_older_images() {
        let mut image_message = crate::chat::ChatMessage::user("Reference");
        image_message.images.push(ChatImage {
            file: "fixture.png".into(),
            width: 1,
            height: 1,
        });
        let mut messages = vec![image_message];
        assert!(latest_user_images(&messages).is_some());
        messages.push(crate::chat::ChatMessage::assistant("Reviewed"));
        assert!(latest_user_images(&messages).is_some());
        messages.push(crate::chat::ChatMessage::user("Now just explain"));
        assert!(latest_user_images(&messages).is_none());
    }
}
