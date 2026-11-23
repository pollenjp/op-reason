//! The Start menu's shortcut to the settings window (design `2026-11-23-settings-design.md` §13):
//! `%APPDATA%\Microsoft\Windows\Start Menu\Programs\op-reason.lnk`, which starts `op-reason.exe --settings`.

use std::path::{Path, PathBuf};

use op_reason_core::settings::engine::{ShortcutAct, ShortcutNow, ShortcutState};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWMINNOACTIVE;
use windows::core::{HSTRING, Interface};

/// The shortcut's arguments: the settings window.
const ARGS: &str = "--settings";

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\op-reason.lnk`.
fn path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "APPDATA is not set".to_owned())?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("op-reason.lnk"))
}

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("op-reason.exe: {e}"))
}

fn text(buf: &[u16]) -> String {
    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

/// The target and the arguments of the shortcut at `path` (COM must be initialized).
fn read(path: &Path) -> Result<(String, String), String> {
    // SAFETY: COM calls with buffers that live across them.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("ShellLink: {}", e.message()))?;
        let file: IPersistFile = link.cast().map_err(|e| e.message())?;
        file.Load(&HSTRING::from(path.as_os_str()), STGM_READ)
            .map_err(|e| format!("{}: {}", path.display(), e.message()))?;
        let mut target = vec![0u16; 1024];
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
            .map_err(|e| e.message())?;
        let mut args = vec![0u16; 1024];
        link.GetArguments(&mut args).map_err(|e| e.message())?;
        Ok((text(&target), text(&args)))
    }
}

/// What is there now.
fn look(path: &Path) -> Result<ShortcutState, String> {
    if !path.exists() {
        return Ok(ShortcutState::Missing);
    }
    let Ok((target, args)) = read(path) else {
        return Ok(ShortcutState::Foreign(
            "(not a shortcut that can be read)".into(),
        ));
    };
    let file = Path::new(&target)
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if file != "op-reason.exe" || args.trim() != ARGS {
        return Ok(ShortcutState::Foreign(
            format!("{target} {args}").trim().to_owned(),
        ));
    }
    let me = exe()?;
    if target.eq_ignore_ascii_case(&me.display().to_string()) {
        Ok(ShortcutState::Ok)
    } else {
        Ok(ShortcutState::Differs(target))
    }
}

/// Makes the shortcut for this exe (minimized: its short-lived console only shows in the taskbar for a moment).
fn place(path: &Path) -> Result<(), String> {
    let me = exe()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    // SAFETY: COM calls with strings that live across them.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("ShellLink: {}", e.message()))?;
        link.SetPath(&HSTRING::from(me.as_os_str()))
            .map_err(|e| e.message())?;
        link.SetArguments(&HSTRING::from(ARGS))
            .map_err(|e| e.message())?;
        if let Some(dir) = me.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))
                .map_err(|e| e.message())?;
        }
        link.SetDescription(&HSTRING::from("op-reason settings"))
            .map_err(|e| e.message())?;
        link.SetShowCmd(SW_SHOWMINNOACTIVE)
            .map_err(|e| e.message())?;
        let file: IPersistFile = link.cast().map_err(|e| e.message())?;
        file.Save(&HSTRING::from(path.as_os_str()), true)
            .map_err(|e| format!("{}: {}", path.display(), e.message()))
    }
}

/// Looks at, makes or removes the shortcut. Places and removals refuse a foreign shortcut themselves too.
pub fn act(act: ShortcutAct) -> Result<ShortcutNow, String> {
    let path = path()?;
    let state = look(&path)?;
    match (act, &state) {
        (ShortcutAct::Look, _) => {}
        (_, ShortcutState::Foreign(_)) => {
            return Err(format!("{} is not op-reason's", path.display()));
        }
        (ShortcutAct::Place, _) => place(&path)?,
        (ShortcutAct::Remove, ShortcutState::Missing) => {}
        (ShortcutAct::Remove, _) => {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?
        }
    }
    let state = if act == ShortcutAct::Look {
        state
    } else {
        look(&path)?
    };
    Ok(ShortcutNow {
        state,
        path: path.display().to_string(),
    })
}
