fn main() {
    #[cfg(windows)]
    {
        let rc_file = "assets/icon.ico";
        if std::path::Path::new(rc_file).exists() {
            let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
            let obj_file = format!("{}/echo-app-icon.o", out_dir);
            let windres_cmd = if std::process::Command::new("windres").arg("--version").output().is_ok() {
                Some("windres")
            } else if std::process::Command::new("x86_64-w64-mingw32-windres").arg("--version").output().is_ok() {
                Some("x86_64-w64-mingw32-windres")
            } else {
                None
            };

            if let Some(cmd) = windres_cmd {
                if let Ok(canon) = std::fs::canonicalize(rc_file) {
                    let clean_path = canon.display().to_string().replace(r"\\?\", "").replace('\\', "/");
                    let rc_content = format!("1 ICON \"{}\"\n", clean_path);
                    let rc_path = format!("{}/echo.rc", out_dir);
                    if std::fs::write(&rc_path, rc_content).is_ok() {
                        if let Ok(status) = std::process::Command::new(cmd)
                            .args(["-i", &rc_path, "-o", &obj_file])
                            .status()
                        {
                            if status.success() {
                                println!("cargo:rustc-link-arg={}", obj_file);
                            }
                        }
                    }
                }
            }
        }
    }
}
