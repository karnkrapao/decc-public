use std::{ffi::OsStr, process::Command};

#[cfg(windows)]
const WINDOWS_BACKGROUND_CREATION_FLAGS: u32 =
    windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

pub(crate) fn background_command<S>(program: S) -> Command
where
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_BACKGROUND_CREATION_FLAGS);
    }

    command
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn windows_background_commands_use_create_no_window() {
        assert_eq!(
            super::WINDOWS_BACKGROUND_CREATION_FLAGS,
            windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_background_child_has_no_console_window() {
        let script = r#"
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class NativeConsole { [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); }';
[Console]::Out.Write([NativeConsole]::GetConsoleWindow().ToInt64())
"#;
        let output = super::background_command("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .expect("spawn hidden PowerShell probe");

        assert!(
            output.status.success(),
            "hidden PowerShell probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "0",
            "background child unexpectedly received a console window"
        );
    }
}
