use std::fs;
use std::path::{Path, PathBuf};

use crosspond_model::{ImagePart, ImageSource};

use crate::context::{MAX_STAGED_FILE_BYTES, StagedInput, unique_file_name};

/// Cap on composer attachments per turn.
pub const MAX_ATTACHMENTS: usize = 8;

/// Skip copying oversized user videos into `input/`.
pub const MAX_STAGED_VIDEO_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentKind {
    Image,
    Video,
}

/// User-attached image or video for one turn.
///
/// `bytes` are vision bytes (the image, or a video poster JPEG). `source_path`
/// is the original video file for staging. `Debug` redacts bytes and paths.
#[derive(Clone, Eq, PartialEq)]
pub struct UserAttachment {
    pub name: String,
    pub kind: AttachmentKind,
    pub media_type: String,
    pub bytes: Vec<u8>,
    pub source_path: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl std::fmt::Debug for UserAttachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserAttachment")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("media_type", &self.media_type)
            .field("bytes_len", &self.bytes.len())
            .field("source_path_present", &self.source_path.is_some())
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl UserAttachment {
    pub fn display_name(&self) -> &str {
        let name = self.name.trim();
        if name.is_empty() { "file" } else { name }
    }

    pub fn vision_media_type(&self) -> Option<&'static str> {
        if self.bytes.is_empty() {
            return None;
        }
        match self.kind {
            AttachmentKind::Image => parse_image_media_type(&self.media_type),
            AttachmentKind::Video => {
                if self.bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                    Some("image/png")
                } else {
                    Some("image/jpeg")
                }
            }
        }
    }
}

pub fn parse_image_media_type(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn parse_video_media_type(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "video/mp4" => Some("video/mp4"),
        "video/quicktime" => Some("video/quicktime"),
        "video/webm" => Some("video/webm"),
        "video/x-m4v" | "video/mp4v-es" => Some("video/x-m4v"),
        _ => None,
    }
}

pub fn kind_from_name(name: &str) -> Option<AttachmentKind> {
    let ext = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())?;
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" => Some(AttachmentKind::Image),
        "mp4" | "mov" | "m4v" | "webm" => Some(AttachmentKind::Video),
        _ => None,
    }
}

pub fn sanitize_file_name(name: &str) -> String {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let cleaned: String = file_name
        .chars()
        .filter(|ch| *ch != '/' && *ch != '\\' && *ch != '\0')
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "file".into()
    } else {
        cleaned
    }
}

pub fn pasted_image(media_type: &str, bytes: Vec<u8>) -> Result<UserAttachment, String> {
    let media_type =
        parse_image_media_type(media_type).ok_or_else(|| "unsupported image type".to_string())?;
    if bytes.is_empty() {
        return Err("image was empty".into());
    }
    if bytes.len() as u64 > MAX_STAGED_FILE_BYTES {
        return Err("image is too large".into());
    }
    let name = match media_type {
        "image/png" => "Pasted image.png",
        "image/gif" => "Pasted image.gif",
        "image/webp" => "Pasted image.webp",
        _ => "Pasted image.jpg",
    };
    Ok(UserAttachment {
        name: name.into(),
        kind: AttachmentKind::Image,
        media_type: media_type.into(),
        bytes,
        source_path: None,
        width: None,
        height: None,
    })
}

pub fn stage_user_attachments(
    input_dir: &Path,
    attachments: &[UserAttachment],
) -> Vec<StagedInput> {
    let mut staged = Vec::new();
    let _ = fs::create_dir_all(input_dir);
    for attachment in attachments.iter().take(MAX_ATTACHMENTS) {
        let file_name = unique_file_name(input_dir, &sanitize_file_name(attachment.display_name()));
        let dest = input_dir.join(&file_name);
        let wrote = match attachment.kind {
            AttachmentKind::Image => write_image(attachment, &dest),
            AttachmentKind::Video => copy_video(attachment, &dest),
        };
        if wrote {
            staged.push(StagedInput {
                original: PathBuf::from(&file_name),
                relative: format!("input/{file_name}"),
                user_attached: true,
            });
        }
    }
    staged
}

fn write_image(attachment: &UserAttachment, dest: &Path) -> bool {
    if attachment.bytes.is_empty() || attachment.bytes.len() as u64 > MAX_STAGED_FILE_BYTES {
        return false;
    }
    fs::write(dest, &attachment.bytes).is_ok()
}

fn copy_video(attachment: &UserAttachment, dest: &Path) -> bool {
    let Some(path) = &attachment.source_path else {
        return false;
    };
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_STAGED_VIDEO_BYTES {
        return false;
    }
    fs::copy(path, dest).is_ok()
}

/// Routing for the system / user prompt. Names and `input/` relatives only.
pub fn routing(attachments: &[UserAttachment], staged: &[StagedInput]) -> String {
    if attachments.is_empty() {
        return String::new();
    }
    let mut lines = vec!["User attachments for this turn (explicit; honor them):".to_string()];
    for attachment in attachments.iter().take(MAX_ATTACHMENTS) {
        match attachment.kind {
            AttachmentKind::Image => {
                lines.push(format!(
                    "- {} is attached as an image. Look at that image before answering.",
                    attachment.display_name()
                ));
            }
            AttachmentKind::Video => {
                let relative =
                    staged_relative(staged, attachment.display_name()).unwrap_or("input/");
                lines.push(format!(
                    "- {} is a video. You cannot play video. A still frame is attached when available. Use {relative} if you need the original file.",
                    attachment.display_name()
                ));
            }
        }
    }
    lines.join("\n")
}

fn staged_relative<'a>(staged: &'a [StagedInput], name: &str) -> Option<&'a str> {
    let file_name = sanitize_file_name(name);
    staged.iter().find_map(|item| {
        if !item.user_attached {
            return None;
        }
        Path::new(&item.relative)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| *name == file_name)
            .map(|_| item.relative.as_str())
    })
}

pub fn display_names(attachments: &[UserAttachment]) -> Vec<String> {
    attachments
        .iter()
        .take(MAX_ATTACHMENTS)
        .map(|attachment| attachment.display_name().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

pub fn vision_parts(attachments: &[UserAttachment]) -> Vec<ImagePart> {
    attachments
        .iter()
        .take(MAX_ATTACHMENTS)
        .filter_map(|attachment| {
            let media_type = attachment.vision_media_type()?.to_string();
            Some(ImagePart {
                media_type,
                bytes: attachment.bytes.clone(),
                width: attachment.width,
                height: attachment.height,
                source: ImageSource::Attachment,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_bytes_and_paths() {
        let attachment = UserAttachment {
            name: "secret.png".into(),
            kind: AttachmentKind::Image,
            media_type: "image/png".into(),
            bytes: b"PNGSECRET".to_vec(),
            source_path: Some(PathBuf::from("/Users/me/secret.png")),
            width: None,
            height: None,
        };
        let rendered = format!("{attachment:?}");
        assert!(rendered.contains("secret.png"));
        assert!(!rendered.contains("PNGSECRET"));
        assert!(!rendered.contains("/Users/me"));
        assert!(rendered.contains("source_path_present: true"));
    }

    #[test]
    fn pasted_image_rejects_unsupported_and_huge() {
        assert!(pasted_image("image/svg+xml", vec![1, 2, 3]).is_err());
        let huge = vec![0u8; (MAX_STAGED_FILE_BYTES as usize) + 1];
        assert!(pasted_image("image/png", huge).is_err());
        let ok = pasted_image("image/jpg", vec![1, 2, 3]).unwrap();
        assert_eq!(ok.media_type, "image/jpeg");
        assert_eq!(ok.name, "Pasted image.jpg");
    }

    #[test]
    fn stages_image_bytes_without_original_path() {
        let root = std::env::temp_dir().join(format!("crosspond-attach-{}", uuid::Uuid::new_v4()));
        let input = root.join("input");
        fs::create_dir_all(&input).unwrap();
        let attachment = UserAttachment {
            name: "shot.png".into(),
            kind: AttachmentKind::Image,
            media_type: "image/png".into(),
            bytes: b"png-bytes".to_vec(),
            source_path: Some(PathBuf::from("/Users/me/Desktop/shot.png")),
            width: None,
            height: None,
        };
        let staged = stage_user_attachments(&input, &[attachment]);
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].relative, "input/shot.png");
        assert!(staged[0].user_attached);
        assert!(!staged[0].original.is_absolute());
        assert_eq!(
            fs::read_to_string(input.join("shot.png")).unwrap(),
            "png-bytes"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stages_video_from_source_path() {
        let root = std::env::temp_dir().join(format!("crosspond-video-{}", uuid::Uuid::new_v4()));
        let input = root.join("input");
        fs::create_dir_all(&input).unwrap();
        let source = root.join("clip.mov");
        fs::write(&source, "movie-bytes").unwrap();
        let attachment = UserAttachment {
            name: "clip.mov".into(),
            kind: AttachmentKind::Video,
            media_type: "video/quicktime".into(),
            bytes: b"poster".to_vec(),
            source_path: Some(source),
            width: None,
            height: None,
        };
        let staged = stage_user_attachments(&input, &[attachment.clone()]);
        assert_eq!(staged[0].relative, "input/clip.mov");
        assert_eq!(
            fs::read_to_string(input.join("clip.mov")).unwrap(),
            "movie-bytes"
        );
        let text = routing(&[attachment], &staged);
        assert!(text.contains("clip.mov"));
        assert!(text.contains("input/clip.mov"));
        assert!(text.contains("cannot play video"));
        assert!(!text.contains(&root.display().to_string()));
        let decoy = StagedInput {
            original: PathBuf::from("my-clip.mov"),
            relative: "input/my-clip.mov".into(),
            user_attached: true,
        };
        let clipped = UserAttachment {
            name: "clip.mov".into(),
            kind: AttachmentKind::Video,
            media_type: "video/quicktime".into(),
            bytes: Vec::new(),
            source_path: None,
            width: None,
            height: None,
        };
        let exact = routing(&[clipped], &[decoy, staged[0].clone()]);
        assert!(exact.contains("input/clip.mov"));
        assert!(!exact.contains("input/my-clip.mov"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sanitize_drops_path_components() {
        assert_eq!(sanitize_file_name("/tmp/../secret.png"), "secret.png");
        assert_eq!(sanitize_file_name(".."), "file");
    }
}
