//! The user's image editor: detection of common ones, any program they pick, and launching.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum Launch {
    /// Run the program with the image path as its argument (works for nearly every editor).
    Exe(PathBuf),
    /// A URI protocol the editor registers (Store-installed Paint.NET).
    Protocol(String),
    /// Whatever Windows' "Edit" action is for the file type.
    EditVerb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub name: String,
    pub launch: Launch,
}

impl Editor {
    pub fn windows_default() -> Self {
        Self { name: "Windows' editor".into(), launch: Launch::EditVerb }
    }

    pub fn from_exe(exe: &Path) -> Self {
        let name = file_description(exe).unwrap_or_else(|| {
            let stem = exe.file_stem().unwrap_or_default().to_string_lossy();
            let mut chars = stem.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        });
        Self { name, launch: Launch::Exe(exe.to_path_buf()) }
    }

    pub fn detail(&self) -> String {
        match &self.launch {
            Launch::Exe(p) => p.display().to_string(),
            Launch::Protocol(p) => format!("{p} (Microsoft Store)"),
            Launch::EditVerb => "The program Windows uses for Edit".into(),
        }
    }

    pub fn open(&self, image: &Path) -> Result<(), String> {
        match &self.launch {
            Launch::Exe(exe) => {
                Command::new(exe).arg(image).spawn().map(drop).map_err(|e| format!("Couldn't start {}: {e}", self.name))
            }
            Launch::Protocol(prefix) => crate::platform::shell_execute("open", &format!("{prefix}{}", image.display())),
            Launch::EditVerb => crate::platform::shell_execute("edit", &image.display().to_string())
                .map_err(|_| "Windows has no editor set for this file type".into()),
        }
    }

    /// One line for settings storage.
    pub fn to_storage(&self) -> String {
        match &self.launch {
            Launch::Exe(p) => format!("exe|{}|{}", self.name, p.display()),
            Launch::Protocol(p) => format!("protocol|{}|{p}", self.name),
            Launch::EditVerb => "edit".into(),
        }
    }

    pub fn from_storage(s: &str) -> Option<Self> {
        let mut parts = s.splitn(3, '|');
        match (parts.next()?, parts.next(), parts.next()) {
            ("exe", Some(name), Some(path)) => {
                let path = PathBuf::from(path);
                path.is_file().then(|| Self { name: name.into(), launch: Launch::Exe(path) })
            }
            ("protocol", Some(name), Some(p)) => Some(Self { name: name.into(), launch: Launch::Protocol(p.into()) }),
            ("edit", ..) => Some(Self::windows_default()),
            _ => None,
        }
    }
}

/// Image editors installed on this machine, best-known first.
pub fn detect() -> Vec<Editor> {
    let program_files = env_path("ProgramFiles");
    let program_files_x86 = env_path("ProgramFiles(x86)");
    let local = env_path("LOCALAPPDATA");
    let steam = steam_libraries();
    let in_steam =
        |rel: &str| steam.iter().map(|lib| lib.join("steamapps").join("common").join(rel)).collect::<Vec<_>>();
    let under = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel)).into_iter().collect::<Vec<_>>();

    let find = |app_path: &str, candidates: Vec<PathBuf>| {
        app_paths(app_path).into_iter().chain(candidates).find(|p| p.is_file())
    };
    let mut found = Vec::new();
    let add = |found: &mut Vec<Editor>, name: &str, exe: Option<PathBuf>| {
        if let Some(exe) = exe {
            found.push(Editor { name: name.into(), launch: Launch::Exe(exe) });
        }
    };

    match find("PaintDotNet.exe", under(&program_files, r"paint.net\paintdotnet.exe")) {
        Some(exe) => add(&mut found, "Paint.NET", Some(exe)),
        None if has_protocol("paintdotnet") => {
            found.push(Editor { name: "Paint.NET".into(), launch: Launch::Protocol("paintdotnet:".into()) });
        }
        None => {}
    }
    add(
        &mut found,
        "Aseprite",
        find(
            "Aseprite.exe",
            [under(&program_files, r"Aseprite\Aseprite.exe"), in_steam(r"Aseprite\Aseprite.exe")].concat(),
        ),
    );
    add(&mut found, "Krita", find("krita.exe", under(&program_files, r"Krita (x64)in\krita.exe")));
    add(&mut found, "GIMP", find("gimp.exe", gimp(&program_files)));
    add(&mut found, "Adobe Photoshop", find("Photoshop.exe", Vec::new()));
    add(
        &mut found,
        "Affinity Photo",
        find(
            "Photo.exe",
            [under(&program_files, r"Affinity\Photo 2\Photo.exe"), under(&program_files, r"Affinity\Photo\Photo.exe")]
                .concat(),
        ),
    );
    add(&mut found, "Pixelorama", find("Pixelorama.exe", in_steam(r"Pixelorama\Pixelorama.exe")));
    add(&mut found, "Pyxel Edit", find("PyxelEdit.exe", under(&program_files_x86, r"PyxelEdit\PyxelEdit.exe")));
    add(&mut found, "Paint", find("mspaint.exe", under(&local, r"Microsoft\WindowsApps\mspaint.exe")));
    found
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// The App Paths registration an installer writes for its executable.
fn app_paths(exe: &str) -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let key = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| RegKey::predef(root).open_subkey(&key).ok()?.get_value::<String, _>("").ok())
        .map(|v| PathBuf::from(v.trim_matches('"')))
        .collect()
}

fn has_protocol(name: &str) -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT).open_subkey(name).is_ok()
}

/// GIMP installs into a versioned folder ("GIMP 2", "GIMP 3") with a versioned exe.
fn gimp(program_files: &Option<PathBuf>) -> Vec<PathBuf> {
    let Some(pf) = program_files else { return Vec::new() };
    let Ok(dirs) = std::fs::read_dir(pf) else { return Vec::new() };
    let mut out = Vec::new();
    for dir in dirs.filter_map(Result::ok) {
        if !dir.file_name().to_string_lossy().starts_with("GIMP") {
            continue;
        }
        if let Ok(bins) = std::fs::read_dir(dir.path().join("bin")) {
            for bin in bins.filter_map(Result::ok) {
                let name = bin.file_name().to_string_lossy().to_lowercase();
                if name.starts_with("gimp-")
                    && name.ends_with(".exe")
                    && !name.contains("console")
                    && !name.contains("debug")
                {
                    out.push(bin.path());
                }
            }
        }
    }
    out.sort();
    out.reverse(); // newest version first
    out
}

/// Steam library folders, for editors bought on Steam (Aseprite, Pixelorama).
fn steam_libraries() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let Ok(steam) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") else { return Vec::new() };
    let Ok(root) = steam.get_value::<String, _>("SteamPath") else { return Vec::new() };
    let root = PathBuf::from(root);
    let mut libs = vec![root.clone()];
    if let Ok(vdf) = std::fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) {
        for line in vdf.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("\"path\"") {
                let path = rest.trim().trim_matches('"').replace("\\\\", "\\");
                libs.push(PathBuf::from(path));
            }
        }
    }
    libs.sort();
    libs.dedup();
    libs
}

/// The program's own name from its version resource ("Affinity Photo 2" rather than "Photo").
fn file_description(exe: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let path = wide(exe.as_os_str());
    // SAFETY: buffers are sized by the API and outlive every pointer taken into them.
    unsafe {
        let size = GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let query = wide(std::ffi::OsStr::new(r"\VarFileInfo\Translation"));
        if VerQueryValueW(data.as_ptr().cast(), query.as_ptr(), &mut ptr, &mut len) == 0 || len < 4 {
            return None;
        }
        let lang = *(ptr as *const u16);
        let codepage = *(ptr as *const u16).add(1);
        let key = format!(r"\StringFileInfo\{lang:04x}{codepage:04x}\FileDescription");
        let query = wide(std::ffi::OsStr::new(&key));
        if VerQueryValueW(data.as_ptr().cast(), query.as_ptr(), &mut ptr, &mut len) == 0 || len == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_round_trip() {
        let exe = std::env::current_exe().unwrap();
        for editor in [
            Editor { name: "Aseprite".into(), launch: Launch::Exe(exe.clone()) },
            Editor { name: "Paint.NET".into(), launch: Launch::Protocol("paintdotnet:".into()) },
            Editor::windows_default(),
        ] {
            assert_eq!(Editor::from_storage(&editor.to_storage()), Some(editor));
        }
    }

    #[test]
    fn a_vanished_program_is_forgotten() {
        assert_eq!(Editor::from_storage(r"exe|Gone|C:\nope\gone.exe"), None);
    }

    #[test]
    fn names_come_from_version_info() {
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        if notepad.is_file() {
            assert!(!Editor::from_exe(notepad).name.is_empty());
        }
    }
}
