//! RightType application-data directory selection.

use std::path::PathBuf;

pub fn righttype_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("RIGHTTYPE_E2E_DATA_DIR") {
        return Some(PathBuf::from(path));
    }

    let mut path = PathBuf::from(std::env::var_os("APPDATA")?);
    path.push("RightType");
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_directory_has_righttype_leaf() {
        if std::env::var_os("RIGHTTYPE_E2E_DATA_DIR").is_none() {
            assert_eq!(
                righttype_dir().and_then(|path| path.file_name().map(|s| s.to_owned())),
                Some("RightType".into())
            );
        }
    }
}
