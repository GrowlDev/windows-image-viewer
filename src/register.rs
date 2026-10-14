//! Registers the viewer with Windows so it appears in Default apps and "Open with".
//!
//! Everything lives under HKEY_CURRENT_USER: no admin rights, nothing system-wide, and
//! `--unregister` removes it all. Windows doesn't let apps make themselves the default;
//! the user picks it in Settings > Apps > Default apps.

use std::path::Path;

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};

use crate::folder::EXTENSIONS;

pub const APP_NAME: &str = "Windows Image Viewer";
const PROG_ID: &str = "WindowsImageViewer.Image";
const CAPABILITIES: &str = r"Software\WindowsImageViewer\Capabilities";
const APP_ROOT: &str = r"Software\WindowsImageViewer";

fn exe_name(exe: &Path) -> String {
    exe.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

pub fn register(exe: &Path) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;
    let exe = exe.display().to_string();
    let open = format!("\"{exe}\" \"%1\"");
    let icon = format!("\"{exe}\",0");

    let (prog, _) = classes.create_subkey(PROG_ID)?;
    prog.set_value("", &"Image")?;
    prog.create_subkey("DefaultIcon")?.0.set_value("", &icon)?;
    prog.create_subkey(r"shell\open\command")?.0.set_value("", &open)?;

    let (app, _) = classes.create_subkey(format!(r"Applications\{}", exe_name(Path::new(&exe))))?;
    app.set_value("FriendlyAppName", &APP_NAME)?;
    app.create_subkey(r"shell\open\command")?.0.set_value("", &open)?;
    let (types, _) = app.create_subkey("SupportedTypes")?;

    let (caps, _) = hkcu.create_subkey(CAPABILITIES)?;
    caps.set_value("ApplicationName", &APP_NAME)?;
    caps.set_value("ApplicationIcon", &icon)?;
    caps.set_value("ApplicationDescription", &"A fast, minimal viewer for pixel art and photos.")?;
    let (assoc, _) = caps.create_subkey("FileAssociations")?;

    for ext in EXTENSIONS {
        let dot = format!(".{ext}");
        types.set_value(&dot, &"")?;
        assoc.set_value(&dot, &PROG_ID)?;
        classes.create_subkey(format!(r"{dot}\OpenWithProgids"))?.0.set_value(PROG_ID, &"")?;
    }

    hkcu.create_subkey(r"Software\RegisteredApplications")?
        .0
        .set_value(APP_NAME, &CAPABILITIES)?;
    notify_shell();
    Ok(())
}

pub fn unregister(exe: &Path) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;
    let _ = classes.delete_subkey_all(PROG_ID);
    let _ = classes.delete_subkey_all(format!(r"Applications\{}", exe_name(exe)));
    for ext in EXTENSIONS {
        if let Ok(key) = classes.open_subkey_with_flags(format!(r".{ext}\OpenWithProgids"), KEY_ALL_ACCESS) {
            let _ = key.delete_value(PROG_ID);
        }
    }
    let _ = hkcu.delete_subkey_all(APP_ROOT);
    if let Ok(key) = hkcu.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_ALL_ACCESS) {
        let _ = key.delete_value(APP_NAME);
    }
    notify_shell();
    Ok(())
}

fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    // SAFETY: documented call with no item arguments.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };
}
