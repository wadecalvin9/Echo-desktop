fn main() {
    #[cfg(windows)]
    {
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        if !out_dir.is_empty() {
            let out_path = std::path::PathBuf::from(&out_dir);
            if let Some(target_dir) = out_path.ancestors().nth(3) {
                copy_required_dlls(target_dir);
            }
        }
    }
}

#[cfg(windows)]
fn copy_required_dlls(target_dir: &std::path::Path) {
    // 1. libunwind.dll from llvm-mingw-ucrt
    let libunwind_candidates = [
        std::path::PathBuf::from(r"C:\Users\Administrator\llvm-mingw-ucrt\bin\libunwind.dll"),
        std::path::PathBuf::from(r"C:\Users\Administrator\llvm-mingw-ucrt\x86_64-w64-mingw32\bin\libunwind.dll"),
    ];
    for cand in &libunwind_candidates {
        if cand.exists() {
            let dest = target_dir.join("libunwind.dll");
            let _ = std::fs::copy(cand, dest);
            break;
        }
    }

    // 2. WebView2Loader.dll
    let dest_wv2 = target_dir.join("WebView2Loader.dll");
    let build_dir = target_dir.join("build");
    if let Ok(entries) = std::fs::read_dir(&build_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("webview2-com-sys-")) {
                let candidate = p.join("out").join("x64").join("WebView2Loader.dll");
                if candidate.exists() {
                    let _ = std::fs::copy(candidate, &dest_wv2);
                    break;
                }
            }
        }
    }
}

