//! The list of viewable images in the current image's folder, in Explorer's order.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Extensions the decoder understands (lower case, no dot).
pub const EXTENSIONS: &[&str] =
    &["png", "apng", "jpg", "jpeg", "jpe", "jfif", "gif", "bmp", "dib", "webp", "ico", "tif", "tiff", "tga", "qoi"];

pub fn is_supported(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Supported images in `dir`, sorted the way Explorer sorts names.
pub fn scan(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| is_supported(p))
        .collect();
    sort_like_explorer(&mut files);
    files
}

/// Sort by name the way Explorer does, encoding each name once rather than per comparison.
#[cfg(windows)]
fn sort_like_explorer(files: &mut Vec<PathBuf>) {
    use windows_sys::Win32::UI::Shell::StrCmpLogicalW;
    let mut keyed: Vec<(Vec<u16>, PathBuf)> =
        files.drain(..).map(|p| (file_name(&p).encode_utf16().chain(std::iter::once(0)).collect(), p)).collect();
    // SAFETY: both keys are NUL-terminated UTF-16 strings owned by `keyed`.
    keyed.sort_by(|a, b| unsafe { StrCmpLogicalW(a.0.as_ptr(), b.0.as_ptr()) }.cmp(&0));
    files.extend(keyed.into_iter().map(|(_, p)| p));
}

#[cfg(not(windows))]
fn sort_like_explorer(files: &mut [PathBuf]) {
    files.sort_by(|a, b| explorer_cmp(file_name(a), file_name(b)));
}

pub fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// Explorer's own "logical" name comparison (frame2 < frame10, case-insensitive).
#[cfg(windows)]
pub fn explorer_cmp(a: &str, b: &str) -> Ordering {
    use windows_sys::Win32::UI::Shell::StrCmpLogicalW;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (a, b) = (wide(a), wide(b));
    // SAFETY: both buffers are NUL-terminated UTF-16 strings that outlive the call.
    unsafe { StrCmpLogicalW(a.as_ptr(), b.as_ptr()) }.cmp(&0)
}

#[cfg(not(windows))]
pub fn explorer_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// Where `path` sits in `files`. Exact matches are the norm (paths come from the same scan);
/// otherwise compare case-insensitively like the file system.
pub fn position(files: &[PathBuf], path: &Path) -> Option<usize> {
    files.iter().position(|p| p == path).or_else(|| {
        let want = path.to_string_lossy().to_lowercase();
        files.iter().position(|p| p.to_string_lossy().to_lowercase() == want)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_sort_naturally() {
        let mut names = vec!["frame10.png", "Frame2.png", "frame1.png", "b.png", "a.png"];
        names.sort_by(|a, b| explorer_cmp(a, b));
        assert_eq!(names, ["a.png", "b.png", "frame1.png", "Frame2.png", "frame10.png"]);
    }

    #[test]
    fn extension_check_ignores_case() {
        assert!(is_supported(Path::new("C:/x/Sprite.PNG")));
        assert!(is_supported(Path::new("photo.JPEG")));
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(!is_supported(Path::new("png")));
    }

    #[test]
    fn scan_lists_only_images_in_order() {
        let dir = std::env::temp_dir().join(format!("imageviewer-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub.png")).unwrap();
        for name in ["b10.png", "b9.gif", "readme.txt", "a.JPG"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let names: Vec<String> = scan(&dir).iter().map(|p| file_name(p).to_owned()).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["a.JPG", "b9.gif", "b10.png"]);
    }
}
