use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Some(Cow::Borrowed(match path {
            "icons/file.svg" => include_bytes!("../assets/icons/file.svg").as_slice(),
            "icons/pencil.svg" => include_bytes!("../assets/icons/pencil.svg").as_slice(),
            "icons/folder.svg" => include_bytes!("../assets/icons/folder.svg").as_slice(),
            "icons/monitor.svg" => include_bytes!("../assets/icons/monitor.svg").as_slice(),
            "icons/pointer.svg" => include_bytes!("../assets/icons/pointer.svg").as_slice(),
            "icons/text.svg" => include_bytes!("../assets/icons/text.svg").as_slice(),
            "icons/search.svg" => include_bytes!("../assets/icons/search.svg").as_slice(),
            "icons/wrench.svg" => include_bytes!("../assets/icons/wrench.svg").as_slice(),
            _ => return Ok(None),
        })))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.trim_end_matches('/') == "icons" {
            Ok(vec![
                "file.svg".into(),
                "pencil.svg".into(),
                "folder.svg".into(),
                "monitor.svg".into(),
                "pointer.svg".into(),
                "text.svg".into(),
                "search.svg".into(),
                "wrench.svg".into(),
            ])
        } else {
            Ok(Vec::new())
        }
    }
}
