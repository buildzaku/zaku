use gpui::{App, SharedString};
use std::path::Path;

const FILE_SUFFIXES_BY_ICON_KEY: &[(&str, &[&str])] =
    &[("json", &["json", "jsonc"]), ("log", &["log"])];

const FILE_ICONS: &[(&str, &str)] = &[
    ("default", "icons/file/file_generic.svg"),
    ("json", "icons/file/code.svg"),
    ("log", "icons/file/info.svg"),
];

#[derive(Debug)]
pub struct FileIcons;

impl FileIcons {
    pub fn get_icon(path: &Path, _: &App) -> Option<SharedString> {
        let get_icon_from_suffix = |suffix: &str| -> Option<SharedString> {
            icon_key_for_suffix(suffix).and_then(icon_for_type)
        };

        if let Some(mut typ) = path.file_name().and_then(|typ| typ.to_str()) {
            let maybe_path = get_icon_from_suffix(typ);
            if maybe_path.is_some() {
                return maybe_path;
            }

            while let Some((_, suffix)) = typ.split_once('.') {
                let maybe_path = get_icon_from_suffix(suffix);
                if maybe_path.is_some() {
                    return maybe_path;
                }
                typ = suffix;
            }
        }

        let extension = path.extension().and_then(|extension| extension.to_str());
        if let Some(extension) = extension {
            let maybe_path = get_icon_from_suffix(extension);
            if maybe_path.is_some() {
                return maybe_path;
            }
        }

        icon_for_type("default")
    }
}

fn icon_key_for_suffix(suffix: &str) -> Option<&'static str> {
    FILE_SUFFIXES_BY_ICON_KEY
        .iter()
        .find_map(|(icon_key, suffixes)| suffixes.contains(&suffix).then_some(*icon_key))
}

fn icon_for_type(typ: &str) -> Option<SharedString> {
    FILE_ICONS
        .iter()
        .find_map(|(icon_type, path)| (*icon_type == typ).then_some((*path).into()))
}
