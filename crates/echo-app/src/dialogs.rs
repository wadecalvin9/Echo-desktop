use std::path::{Path, PathBuf};

pub fn pick_folder(
    owner_hwnd: Option<isize>,
    title: &str,
    initial_dir: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        let title = title.to_string();
        let initial = initial_dir.map(Path::to_path_buf);
        run_sta(move || win32::pick_folder_impl(owner_hwnd, &title, initial.as_deref()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (owner_hwnd, title, initial_dir);
        Ok(None)
    }
}

pub fn pick_backup_save(
    owner_hwnd: Option<isize>,
    initial_path: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        let initial = initial_path.map(Path::to_path_buf);
        run_sta(move || win32::pick_backup_save_impl(owner_hwnd, initial.as_deref()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = owner_hwnd;
        Ok(initial_path.map(Path::to_path_buf))
    }
}

pub fn pick_backup_open(
    owner_hwnd: Option<isize>,
    initial_dir: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        let initial = initial_dir.map(Path::to_path_buf);
        run_sta(move || win32::pick_backup_open_impl(owner_hwnd, initial.as_deref()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (owner_hwnd, initial_dir);
        Ok(None)
    }
}

pub fn pick_playlist_open(
    owner_hwnd: Option<isize>,
    initial_dir: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "windows")]
    {
        let initial = initial_dir.map(Path::to_path_buf);
        run_sta(move || win32::pick_playlist_open_impl(owner_hwnd, initial.as_deref()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (owner_hwnd, initial_dir);
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn run_sta<F>(f: F) -> Result<Option<PathBuf>, String>
where
    F: FnOnce() -> Result<Option<PathBuf>, String> + Send + 'static,
{
    std::thread::spawn(f)
        .join()
        .map_err(|_| "Dialog thread panicked".to_string())?
}

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};
    use windows::core::{Error as WinError, Interface, PCWSTR};
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
        SHCreateItemFromParsingName, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
        SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    fn wide_z(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn resolve_hwnd(owner_hwnd: Option<isize>) -> HWND {
        match owner_hwnd {
            Some(h) if h != 0 => HWND(h),
            _ => unsafe { GetForegroundWindow() },
        }
    }

    struct ComGuard;
    impl ComGuard {
        fn init_sta() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
            hr.ok().map_err(|e| format!("COM init failed: {e}"))?;
            Ok(Self)
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn is_cancelled(err: &WinError) -> bool {
        err.code() == ERROR_CANCELLED.to_hresult()
    }

    fn set_initial_folder(dialog: &IFileDialog, dir: Option<&Path>) {
        let Some(dir) = dir.filter(|d| d.exists()) else {
            return;
        };
        let w = wide_z(&dir.to_string_lossy());
        if let Ok(item) = unsafe { SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(w.as_ptr()), None) } {
            let _ = unsafe { dialog.SetFolder(&item) };
        }
    }

    fn read_result_path(dialog: &IFileDialog, owner: HWND) -> Result<Option<PathBuf>, String> {
        match unsafe { dialog.Show(owner) } {
            Ok(()) => {}
            Err(e) if is_cancelled(&e) => return Ok(None),
            Err(e) => return Err(format!("Dialog failed: {e}")),
        }
        let item = unsafe { dialog.GetResult() }.map_err(|e| format!("No selection: {e}"))?;
        let pwstr = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|e| format!("Path read failed: {e}"))?;
        let s = unsafe { pwstr.to_string() }.map_err(|e| format!("Invalid path: {e}"));
        unsafe { CoTaskMemFree(Some(pwstr.0 as *const c_void)) };
        s.map(|p| Some(PathBuf::from(p)))
    }

    pub fn pick_folder_impl(
        owner_hwnd: Option<isize>,
        title: &str,
        initial_dir: Option<&Path>,
    ) -> Result<Option<PathBuf>, String> {
        let _com = ComGuard::init_sta()?;
        let dlg: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }
            .map_err(|e| format!("Open folder dialog failed: {e}"))?;
        let base: IFileDialog = dlg.cast().map_err(|e| format!("COM cast failed: {e}"))?;
        unsafe {
            let opts = base.GetOptions().unwrap_or_default();
            base.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                .map_err(|e| format!("SetOptions failed: {e}"))?;
            let wtitle = wide_z(title);
            let _ = base.SetTitle(PCWSTR(wtitle.as_ptr()));
        }
        set_initial_folder(&base, initial_dir);
        read_result_path(&base, resolve_hwnd(owner_hwnd))
    }

    pub fn pick_backup_save_impl(
        owner_hwnd: Option<isize>,
        initial_path: Option<&Path>,
    ) -> Result<Option<PathBuf>, String> {
        let _com = ComGuard::init_sta()?;
        let dlg: IFileSaveDialog = unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL) }
            .map_err(|e| format!("Save dialog failed: {e}"))?;
        let base: IFileDialog = dlg.cast().map_err(|e| format!("COM cast failed: {e}"))?;
        let name = wide_z("Echo Backup (*.echobak)");
        let spec = wide_z("*.echobak");
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        let title = wide_z("Export Echo Backup");
        let ext = wide_z("echobak");
        let fname = wide_z(
            initial_path
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("echo-backup.echobak"),
        );
        unsafe {
            let opts = base.GetOptions().unwrap_or_default();
            let _ = base.SetOptions(opts | FOS_FORCEFILESYSTEM);
            let _ = base.SetTitle(PCWSTR(title.as_ptr()));
            let _ = base.SetFileTypes(&filters);
            let _ = base.SetDefaultExtension(PCWSTR(ext.as_ptr()));
            let _ = base.SetFileName(PCWSTR(fname.as_ptr()));
        }
        set_initial_folder(&base, initial_path.and_then(|p| p.parent()));
        read_result_path(&base, resolve_hwnd(owner_hwnd))
    }

    pub fn pick_backup_open_impl(
        owner_hwnd: Option<isize>,
        initial_dir: Option<&Path>,
    ) -> Result<Option<PathBuf>, String> {
        let _com = ComGuard::init_sta()?;
        let dlg: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }
            .map_err(|e| format!("Open dialog failed: {e}"))?;
        let base: IFileDialog = dlg.cast().map_err(|e| format!("COM cast failed: {e}"))?;
        let name = wide_z("Echo Backup (*.echobak)");
        let spec = wide_z("*.echobak");
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(name.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        let title = wide_z("Import Echo Backup");
        unsafe {
            let opts = base.GetOptions().unwrap_or_default();
            let _ = base.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
            let _ = base.SetTitle(PCWSTR(title.as_ptr()));
            let _ = base.SetFileTypes(&filters);
        }
        set_initial_folder(&base, initial_dir);
        read_result_path(&base, resolve_hwnd(owner_hwnd))
    }

    pub fn pick_playlist_open_impl(
        owner_hwnd: Option<isize>,
        initial_dir: Option<&Path>,
    ) -> Result<Option<PathBuf>, String> {
        let _com = ComGuard::init_sta()?;
        let dlg: IFileOpenDialog = unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }
            .map_err(|e| format!("Open dialog failed: {e}"))?;
        let base: IFileDialog = dlg.cast().map_err(|e| format!("COM cast failed: {e}"))?;
        let name = wide_z("Playlists & Libraries (*.csv, *.xml, *.m3u, *.m3u8, *.txt)");
        let spec = wide_z("*.csv;*.xml;*.m3u;*.m3u8;*.txt");
        let all_name = wide_z("All Files (*.*)");
        let all_spec = wide_z("*.*");
        let filters = [
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(name.as_ptr()),
                pszSpec: PCWSTR(spec.as_ptr()),
            },
            COMDLG_FILTERSPEC {
                pszName: PCWSTR(all_name.as_ptr()),
                pszSpec: PCWSTR(all_spec.as_ptr()),
            },
        ];
        let title = wide_z("Import Playlist or Library");
        unsafe {
            let opts = base.GetOptions().unwrap_or_default();
            let _ = base.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
            let _ = base.SetTitle(PCWSTR(title.as_ptr()));
            let _ = base.SetFileTypes(&filters);
        }
        set_initial_folder(&base, initial_dir);
        read_result_path(&base, resolve_hwnd(owner_hwnd))
    }
}

