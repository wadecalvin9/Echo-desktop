//! Google / YouTube Music sign-in helper runner.
//!
//! Spawns the dedicated `echo-auth` helper process so that the WebView2 runtime
//! and Win32 message loop run in complete process isolation, guaranteeing zero
//! symbol or window hook conflicts with Iced/winit.

pub fn launch_google_signin_webview() -> Result<String, String> {
    let mut exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {e}"))?;
    exe_path.set_file_name("echo-auth.exe");

    if !exe_path.exists() {
        // Fallback check in current working directory or target directory
        let fallback = std::path::PathBuf::from("echo-auth.exe");
        let fallback_debug = std::path::PathBuf::from("target/x86_64-pc-windows-gnullvm/debug/echo-auth.exe");
        if fallback.exists() {
            exe_path = fallback;
        } else if fallback_debug.exists() {
            exe_path = fallback_debug;
        } else {
            return Err(format!(
                "Sign-in helper not found at {}. Please ensure echo-auth.exe is built.",
                exe_path.display()
            ));
        }
    }

    let output = std::process::Command::new(&exe_path)
        .output()
        .map_err(|e| format!("Failed to launch sign-in helper: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if err_msg.is_empty() {
            return Err("Sign-in was cancelled or closed.".to_string());
        }
        return Err(format!("Sign-in error: {err_msg}"));
    }

    let cookie = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if cookie.is_empty() {
        return Err("No cookies received from sign-in.".to_string());
    }

    Ok(cookie)
}
