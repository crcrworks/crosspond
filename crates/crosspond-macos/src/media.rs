//! User-picked images and videos for the command bar.

use std::path::{Path, PathBuf};

use crosspond_core::{
    AttachmentKind, MAX_STAGED_FILE_BYTES, MAX_STAGED_VIDEO_BYTES, UserAttachment, kind_from_name,
    sanitize_file_name,
};

/// Open a file sheet for images and videos. Must run on the main thread.
pub fn pick_media_files() -> Result<Vec<PathBuf>, String> {
    #[cfg(not(target_os = "macos"))]
    {
        Err("media picker is only available on macOS".into())
    }
    #[cfg(target_os = "macos")]
    {
        pick_media_files_macos()
    }
}

/// Read a picked file into a user attachment. HEIC becomes JPEG on macOS.
/// Video posters are best-effort.
pub fn prepare_picked_file(path: &Path) -> Result<UserAttachment, String> {
    let name = sanitize_file_name(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
    );
    let kind = kind_from_name(&name).ok_or_else(|| "unsupported file type".to_string())?;
    match kind {
        AttachmentKind::Image => prepare_image(path, name),
        AttachmentKind::Video => prepare_video(path, name),
    }
}

fn prepare_image(path: &Path, name: String) -> Result<UserAttachment, String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "heic" | "heif") {
        return convert_heic(path, &name);
    }
    let metadata = std::fs::metadata(path).map_err(|_| "couldn’t read image".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_STAGED_FILE_BYTES {
        return Err("image is too large".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "couldn’t read image".to_string())?;
    if bytes.is_empty() {
        return Err("image was empty".into());
    }
    let media_type = match ext.as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    Ok(UserAttachment {
        name,
        kind: AttachmentKind::Image,
        media_type: media_type.into(),
        bytes,
        source_path: None,
        width: None,
        height: None,
    })
}

fn prepare_video(path: &Path, name: String) -> Result<UserAttachment, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "couldn’t read video".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_STAGED_VIDEO_BYTES {
        return Err("video is too large".into());
    }
    let media_type = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "webm" => "video/webm",
        "m4v" => "video/x-m4v",
        "mp4" => "video/mp4",
        _ => "video/quicktime",
    };
    Ok(UserAttachment {
        name,
        kind: AttachmentKind::Video,
        media_type: media_type.into(),
        bytes: video_poster_bytes(path).unwrap_or_default(),
        source_path: Some(path.to_path_buf()),
        width: None,
        height: None,
    })
}

fn convert_heic(path: &Path, name: &str) -> Result<UserAttachment, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, name);
        Err("HEIC is only supported on macOS".into())
    }
    #[cfg(target_os = "macos")]
    {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("image");
        let dest =
            std::env::temp_dir().join(format!("crosspond-heic-{}-{stem}.jpg", std::process::id()));
        let status = std::process::Command::new("sips")
            .args([
                "-s",
                "format",
                "jpeg",
                &path.display().to_string(),
                "--out",
                &dest.display().to_string(),
            ])
            .status()
            .map_err(|_| "couldn’t convert HEIC".to_string())?;
        if !status.success() {
            let _ = std::fs::remove_file(&dest);
            return Err("couldn’t convert HEIC".into());
        }
        let bytes = std::fs::read(&dest).map_err(|_| "couldn’t convert HEIC".to_string())?;
        let _ = std::fs::remove_file(&dest);
        if bytes.is_empty() || bytes.len() as u64 > MAX_STAGED_FILE_BYTES {
            return Err("image is too large".into());
        }
        Ok(UserAttachment {
            name: format!("{stem}.jpg"),
            kind: AttachmentKind::Image,
            media_type: "image/jpeg".into(),
            bytes,
            source_path: None,
            width: None,
            height: None,
        })
    }
}

fn video_poster_bytes(path: &Path) -> Option<Vec<u8>> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::temp_dir().join(format!("crosspond-poster-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::process::Command::new("qlmanage")
            .args([
                "-t",
                "-s",
                "768",
                "-o",
                &dir.display().to_string(),
                &path.display().to_string(),
            ])
            .output();
        let file_name = path.file_name()?.to_str()?;
        let png = dir.join(format!("{file_name}.png"));
        let bytes = std::fs::read(&png).ok().filter(|bytes| !bytes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        bytes
    }
}

#[cfg(target_os = "macos")]
fn pick_media_files_macos() -> Result<Vec<PathBuf>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "media picker must run on the main thread".to_string())?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(true);
    let types = NSArray::from_retained_slice(&[
        NSString::from_str("png"),
        NSString::from_str("jpg"),
        NSString::from_str("jpeg"),
        NSString::from_str("gif"),
        NSString::from_str("webp"),
        NSString::from_str("heic"),
        NSString::from_str("heif"),
        NSString::from_str("mp4"),
        NSString::from_str("mov"),
        NSString::from_str("m4v"),
        NSString::from_str("webm"),
    ]);
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&types));
    if panel.runModal() != NSModalResponseOK {
        return Ok(Vec::new());
    }
    let urls = panel.URLs();
    let mut paths = Vec::new();
    for index in 0..urls.count() {
        let url: objc2::rc::Retained<NSURL> = urls.objectAtIndex(index);
        let Some(ns_path) = url.path() else {
            continue;
        };
        let path = PathBuf::from(ns_path.to_string());
        if path.is_file() {
            paths.push(path);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_picker_is_unavailable() {
        #[cfg(not(target_os = "macos"))]
        {
            assert!(pick_media_files().is_err());
            assert!(prepare_picked_file(Path::new("photo.heic")).is_err());
        }
    }

    #[test]
    fn prepares_png_from_path() {
        let root = std::env::temp_dir().join(format!(
            "crosspond-media-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("shot.png");
        std::fs::write(&path, b"png-bytes").unwrap();
        let attachment = prepare_picked_file(&path).unwrap();
        assert_eq!(attachment.kind, AttachmentKind::Image);
        assert_eq!(attachment.media_type, "image/png");
        assert_eq!(attachment.bytes, b"png-bytes");
        assert!(attachment.source_path.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepares_video_without_requiring_a_poster() {
        let root = std::env::temp_dir().join(format!(
            "crosspond-media-vid-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("clip.mov");
        std::fs::write(&path, b"movie-bytes").unwrap();
        let attachment = prepare_picked_file(&path).unwrap();
        assert_eq!(attachment.kind, AttachmentKind::Video);
        assert_eq!(attachment.media_type, "video/quicktime");
        assert_eq!(attachment.source_path.as_deref(), Some(path.as_path()));
        #[cfg(not(target_os = "macos"))]
        assert!(attachment.bytes.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
