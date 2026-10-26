//! OS integration: opening URLs, launching clients and the clipboard.

use std::path::Path;

pub trait Platform: Send + Sync {
    fn open_url(&self, url: &str) -> Result<(), String>;
    fn open_path(&self, path: &Path) -> Result<(), String>;
    fn launch(&self, exe: &str, args: &[String]) -> Result<(), String>;
    fn copy(&self, text: &str) -> Result<(), String>;
}

pub struct SystemPlatform;

impl SystemPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemPlatform {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
fn shell_open(target: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let wide: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!("系统无法打开: {target}"))
    }
}

impl Platform for SystemPlatform {
    fn open_url(&self, url: &str) -> Result<(), String> {
        #[cfg(windows)]
        {
            return shell_open(url);
        }
        #[cfg(not(windows))]
        {
            let _ = url;
            Err("当前平台不支持打开链接".to_string())
        }
    }

    fn open_path(&self, path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        {
            return shell_open(&path.to_string_lossy());
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err("当前平台不支持打开目录".to_string())
        }
    }

    fn launch(&self, exe: &str, args: &[String]) -> Result<(), String> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new(exe)
                .args(args)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = (exe, args);
            Err("当前平台不支持启动客户端".to_string())
        }
    }

    fn copy(&self, text: &str) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_text(text.to_string())
            .map_err(|error| error.to_string())
    }
}
