//! Cross-platform `goz` Windows Service Control Manager (SCM) inspection and UAC lifecycle helpers.

use std::path::{Path, PathBuf};

/// Lifecycle state of the `goz` background indexing Windows service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonServiceState {
    NotInstalled,
    Disabled,
    Stopped,
    Starting,
    Running,
    Stopping,
    Paused,
    UnsupportedPlatform,
}

impl DaemonServiceState {
    /// Human-readable status label suitable for UI display and accessibility text.
    pub fn label(self) -> &'static str {
        match self {
            Self::NotInstalled => "Not Installed",
            Self::Disabled => "Disabled",
            Self::Stopped => "Stopped",
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Stopping => "Stopping",
            Self::Paused => "Paused",
            Self::UnsupportedPlatform => "Windows Only",
        }
    }

    /// Returns `true` when the service is registered in the Windows SCM.
    pub fn is_installed(self) -> bool {
        !matches!(self, Self::NotInstalled | Self::UnsupportedPlatform)
    }

    /// Returns `true` when the service is actively running.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Returns the expected installed location of the elevated service executable (`%ProgramFiles%\goz\gozd.exe`).
pub fn installed_gozd_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        goz_winfs::program_files_dir()
            .ok()
            .map(|pf| pf.join("goz").join("gozd.exe"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Queries the Windows SCM for the `goz` service state without requiring elevation.
///
/// On non-Windows platforms, returns [`DaemonServiceState::UnsupportedPlatform`].
pub fn query_service_state() -> Result<DaemonServiceState, String> {
    #[cfg(target_os = "windows")]
    {
        use goz_core::proto::SERVICE_NAME;
        use goz_winfs::ServiceState;

        let state = goz_winfs::service_state(SERVICE_NAME)
            .map_err(|e| format!("Failed to query Windows service '{SERVICE_NAME}': {e}"))?;
        Ok(match state {
            ServiceState::NotInstalled => DaemonServiceState::NotInstalled,
            ServiceState::Disabled => DaemonServiceState::Disabled,
            ServiceState::Stopped => DaemonServiceState::Stopped,
            ServiceState::Starting => DaemonServiceState::Starting,
            ServiceState::Running => DaemonServiceState::Running,
            ServiceState::Stopping => DaemonServiceState::Stopping,
            ServiceState::Paused => DaemonServiceState::Paused,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(DaemonServiceState::UnsupportedPlatform)
    }
}

/// Installs or upgrades and starts the `goz` Windows service via UAC elevation using `gozd_exe`.
pub fn install_or_upgrade_service(gozd_exe: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if !gozd_exe.is_file() {
            return Err(format!("gozd.exe does not exist at {}", gozd_exe.display()));
        }
        run_elevated_exe(gozd_exe, "install")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = gozd_exe;
        Err("The goz Windows service is only supported on Windows.".to_owned())
    }
}

/// Enables (if disabled) and starts the already-installed `goz` Windows service via UAC elevation.
pub fn start_service() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use goz_core::proto::SERVICE_NAME;
        let inner = format!(
            "Set-Service -Name {svc} -StartupType Automatic -ErrorAction Stop; Start-Service -Name {svc} -ErrorAction Stop",
            svc = ps_single_quote(SERVICE_NAME)
        );
        run_elevated_powershell_script(&inner)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("The goz Windows service is only supported on Windows.".to_owned())
    }
}

/// Stops and removes the `goz` Windows service and its `Program Files\goz\gozd.exe` copy via UAC.
pub fn uninstall_service(gozd_exe: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if gozd_exe.is_file() {
            return run_elevated_exe(gozd_exe, "uninstall");
        }
        if let Some(installed) = installed_gozd_path()
            && installed.is_file()
        {
            return run_elevated_exe(&installed, "uninstall");
        }
        use goz_core::proto::SERVICE_NAME;
        let inner = format!(
            "Stop-Service -Name {svc} -Force -ErrorAction SilentlyContinue; & sc.exe delete {svc} | Out-Null",
            svc = ps_single_quote(SERVICE_NAME)
        );
        run_elevated_powershell_script(&inner)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = gozd_exe;
        Err("The goz Windows service is only supported on Windows.".to_owned())
    }
}

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
fn ps_single_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn run_elevated_exe(exe: &Path, subcommand: &str) -> Result<(), String> {
    let exe_lit = ps_single_quote(&exe.to_string_lossy());
    let arg_lit = ps_single_quote(subcommand);
    let script = format!(
        concat!(
            "$ErrorActionPreference = 'Stop'; ",
            "try {{ ",
            "  $p = Start-Process -FilePath {exe} -ArgumentList {arg} -Verb RunAs -WindowStyle Hidden -Wait -PassThru; ",
            "  if ($null -eq $p) {{ exit 1 }}; ",
            "  exit $p.ExitCode ",
            "}} catch [System.ComponentModel.Win32Exception] {{ ",
            "  if ($_.Exception.NativeErrorCode -eq 1223) {{ exit 1223 }}; ",
            "  [Console]::Error.WriteLine($_.Exception.Message); ",
            "  exit 1 ",
            "}} catch {{ ",
            "  [Console]::Error.WriteLine($_.Exception.Message); ",
            "  exit 1 ",
            "}}"
        ),
        exe = exe_lit,
        arg = arg_lit,
    );
    execute_hidden_powershell(&script)
}

#[cfg(target_os = "windows")]
fn run_elevated_powershell_script(inner_script: &str) -> Result<(), String> {
    let encoded_inner = encode_utf16le_base64(inner_script);
    let outer = format!(
        concat!(
            "$ErrorActionPreference = 'Stop'; ",
            "try {{ ",
            "  $p = Start-Process -FilePath 'powershell.exe' ",
            "    -ArgumentList @('-NoProfile','-NonInteractive','-WindowStyle','Hidden','-EncodedCommand','{encoded}') ",
            "    -Verb RunAs -WindowStyle Hidden -Wait -PassThru; ",
            "  if ($null -eq $p) {{ exit 1 }}; ",
            "  exit $p.ExitCode ",
            "}} catch [System.ComponentModel.Win32Exception] {{ ",
            "  if ($_.Exception.NativeErrorCode -eq 1223) {{ exit 1223 }}; ",
            "  [Console]::Error.WriteLine($_.Exception.Message); ",
            "  exit 1 ",
            "}} catch {{ ",
            "  [Console]::Error.WriteLine($_.Exception.Message); ",
            "  exit 1 ",
            "}}"
        ),
        encoded = encoded_inner,
    );
    execute_hidden_powershell(&outer)
}

#[cfg(target_os = "windows")]
fn execute_hidden_powershell(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let encoded = encode_utf16le_base64(script);
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Failed to launch PowerShell elevation helper: {e}"))?;

    match output.status.code() {
        Some(0) => Ok(()),
        Some(1223) => Err("Administrator permission prompt (UAC) was cancelled.".to_owned()),
        other => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if stderr.is_empty() {
                Err(format!(
                    "Elevated service command failed (exit code: {})",
                    other.map_or_else(|| "unknown".to_owned(), |c| c.to_string())
                ))
            } else {
                Err(stderr)
            }
        }
    }
}

/// Encodes `script` as UTF-16LE Base64 for PowerShell `-EncodedCommand` (no external crate needed).
fn encode_utf16le_base64(script: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        out.push(TABLE[b0 >> 2] as char);
        out.push(TABLE[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((b1 & 0x0F) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[b2 & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16le_base64_matches_powershell_encoding() {
        // UTF-16LE of "abc" is [0x61, 0x00, 0x62, 0x00, 0x63, 0x00] -> "YQBiAGMA"
        assert_eq!(encode_utf16le_base64("abc"), "YQBiAGMA");
        assert_eq!(encode_utf16le_base64(""), "");
        assert_eq!(encode_utf16le_base64("A"), "QQA=");
    }

    #[test]
    fn service_state_predicates_are_consistent() {
        assert!(!DaemonServiceState::NotInstalled.is_installed());
        assert!(!DaemonServiceState::UnsupportedPlatform.is_installed());
        assert!(DaemonServiceState::Stopped.is_installed());
        assert!(DaemonServiceState::Running.is_installed());
        assert!(DaemonServiceState::Running.is_running());
        assert!(!DaemonServiceState::Stopped.is_running());
        assert_eq!(DaemonServiceState::NotInstalled.label(), "Not Installed");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn scm_query_succeeds_without_elevation() {
        let state = query_service_state().expect("SCM query should succeed for any user");
        assert_ne!(state, DaemonServiceState::UnsupportedPlatform);
    }
}
