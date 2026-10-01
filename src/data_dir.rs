//! RightType application-data directory selection.

use std::path::PathBuf;

pub fn righttype_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("RIGHTTYPE_E2E_DATA_DIR") {
        return Some(PathBuf::from(path));
    }

    if let Some(dir) = portable_dir() {
        return Some(dir);
    }
    let mut path = PathBuf::from(std::env::var_os("APPDATA")?);
    path.push("RightType");
    Some(path)
}

/// Portable use (from a USB stick, or the zip without installing): a file
/// named `portable` next to `righttype.exe` keeps settings and learned words
/// in a `data` folder beside it, so nothing is written to this PC's
/// `%APPDATA%`.
fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let here = exe.parent()?;
    portable_dir_in(here)
}

fn portable_dir_in(here: &std::path::Path) -> Option<PathBuf> {
    here.join("portable").is_file().then(|| here.join("data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portable_marker_keeps_data_beside_the_program() {
        let dir = std::env::temp_dir().join(format!("rt-portable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(portable_dir_in(&dir), None);
        std::fs::write(dir.join("portable"), "").unwrap();
        assert_eq!(portable_dir_in(&dir), Some(dir.join("data")));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
