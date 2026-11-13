use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    PlayPause,
    Next,
    Previous,
    RestoreWindow,
    Exit,
}

pub struct TrayBridge {
    #[cfg(target_os = "windows")]
    inner: Option<WindowsTray>,
    #[cfg(not(target_os = "windows"))]
    _rx: mpsc::Receiver<TrayAction>,
}

impl TrayBridge {
    pub fn new(main_hwnd: Option<isize>) -> Self {
        #[cfg(target_os = "windows")]
        {
            let inner = WindowsTray::new(main_hwnd);
            Self { inner }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let (_tx, _rx) = mpsc::channel();
            Self { _rx }
        }
    }

    pub fn poll_actions(&self) -> Vec<TrayAction> {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref inner) = self.inner {
                inner.poll_actions()
            } else {
                Vec::new()
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            Vec::new()
        }
    }

    pub fn sync(&self, title: Option<&str>, artist: Option<&str>, is_playing: bool) {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref inner) = self.inner {
                inner.sync(title, artist, is_playing);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (title, artist, is_playing);
        }
    }
}

#[cfg(target_os = "windows")]
struct WindowsTray {
    rx: mpsc::Receiver<TrayAction>,
    cmd_tx: mpsc::Sender<TrayCommand>,
}

#[cfg(target_os = "windows")]
enum TrayCommand {
    UpdateTooltip(String, bool),
    Quit,
}

#[cfg(target_os = "windows")]
impl WindowsTray {
    fn new(main_hwnd: Option<isize>) -> Option<Self> {
        let (action_tx, action_rx) = mpsc::channel::<TrayAction>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<TrayCommand>();

        let _thread = std::thread::Builder::new()
            .name("echo-tray".to_string())
            .spawn(move || {
                run_tray_loop(main_hwnd, action_tx, cmd_rx);
            })
            .ok()?;

        Some(Self {
            rx: action_rx,
            cmd_tx,
        })
    }

    fn poll_actions(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(action) = self.rx.try_recv() {
            out.push(action);
        }
        out
    }

    fn sync(&self, title: Option<&str>, artist: Option<&str>, is_playing: bool) {
        let tip = match (title, artist) {
            (Some(t), Some(a)) => format!("Echo • {} - {}", a, t),
            (Some(t), None) => format!("Echo • {}", t),
            _ => "Echo Desktop".to_string(),
        };
        let _ = self.cmd_tx.send(TrayCommand::UpdateTooltip(tip, is_playing));
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsTray {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(TrayCommand::Quit);
    }
}

#[cfg(target_os = "windows")]
fn run_tray_loop(
    main_hwnd_raw: Option<isize>,
    action_tx: mpsc::Sender<TrayAction>,
    cmd_rx: mpsc::Receiver<TrayCommand>,
) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, LoadIconW, PeekMessageW,
        RegisterClassW, SetForegroundWindow, ShowWindow, TrackPopupMenu, TranslateMessage,
        IDI_APPLICATION, MF_DISABLED, MF_GRAYED, MF_SEPARATOR, MF_STRING, PM_REMOVE, SW_RESTORE,
        TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTALIGN, WM_APP, WM_CLOSE, WM_DESTROY,
        WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW,
        WS_POPUP,
    };

    const WM_TRAYICON: u32 = WM_APP + 101;
    const TRAY_ID: u32 = 1001;

    let class_name: Vec<u16> = "EchoTrayWindowClass\0".encode_utf16().collect();

    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR("EchoTray\0".encode_utf16().collect::<Vec<_>>().as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND(0),
            None,
            None,
            None,
        );

        if hwnd.0 == 0 {
            return;
        }

        let icon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            szTip: [0u16; 128],
            ..Default::default()
        };

        let initial_tip = "Echo Desktop\0".encode_utf16().collect::<Vec<_>>();
        let copy_len = initial_tip.len().min(nid.szTip.len());
        nid.szTip[..copy_len].copy_from_slice(&initial_tip[..copy_len]);

        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut is_playing_state = false;
        let mut now_title = "Echo Desktop".to_string();

        let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
        loop {
            // Process commands from main thread
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    TrayCommand::UpdateTooltip(text, is_playing) => {
                        is_playing_state = is_playing;
                        now_title = text.clone();
                        let mut tip_utf16 = text.encode_utf16().collect::<Vec<_>>();
                        tip_utf16.push(0);
                        let mut new_tip = [0u16; 128];
                        let len = tip_utf16.len().min(new_tip.len());
                        new_tip[..len].copy_from_slice(&tip_utf16[..len]);
                        nid.szTip = new_tip;
                        nid.uFlags = NIF_TIP;
                        Shell_NotifyIconW(NIM_MODIFY, &nid);
                    }
                    TrayCommand::Quit => {
                        Shell_NotifyIconW(NIM_DELETE, &nid);
                        let _ = DestroyWindow(hwnd);
                        return;
                    }
                }
            }

            // Message pump with a short timeout / peek
            if PeekMessageW(&mut msg, HWND(0), 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_CLOSE || msg.message == WM_DESTROY {
                    Shell_NotifyIconW(NIM_DELETE, &nid);
                    break;
                }

                if msg.message == WM_TRAYICON {
                    let lparam = msg.lParam.0 as u32;
                    match lparam {
                        WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                            let _ = action_tx.send(TrayAction::RestoreWindow);
                            if let Some(main_raw) = main_hwnd_raw {
                                let main_h = HWND(main_raw);
                                ShowWindow(main_h, SW_RESTORE);
                                SetForegroundWindow(main_h);
                            }
                        }
                        WM_RBUTTONUP => {
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);

                            let hmenu = CreatePopupMenu().unwrap_or_default();
                            if hmenu.0 != 0 {
                                let mut title_w: Vec<u16> = now_title.encode_utf16().collect();
                                title_w.push(0);
                                let _ = AppendMenuW(hmenu, MF_STRING | MF_DISABLED | MF_GRAYED, 100, PCWSTR(title_w.as_ptr()));

                                let sep_w: Vec<u16> = "\0".encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR(sep_w.as_ptr()));

                                let play_pause_label = if is_playing_state {
                                    "Pause\0"
                                } else {
                                    "Play\0"
                                };
                                let play_w: Vec<u16> = play_pause_label.encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_STRING, 101, PCWSTR(play_w.as_ptr()));

                                let next_w: Vec<u16> = "Next Track\0".encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_STRING, 102, PCWSTR(next_w.as_ptr()));

                                let prev_w: Vec<u16> = "Previous Track\0".encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_STRING, 103, PCWSTR(prev_w.as_ptr()));

                                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR(sep_w.as_ptr()));

                                let open_w: Vec<u16> = "Open Echo\0".encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_STRING, 104, PCWSTR(open_w.as_ptr()));

                                let exit_w: Vec<u16> = "Exit\0".encode_utf16().collect();
                                let _ = AppendMenuW(hmenu, MF_STRING, 105, PCWSTR(exit_w.as_ptr()));

                                SetForegroundWindow(hwnd);
                                let cmd = TrackPopupMenu(
                                    hmenu,
                                    TPM_RETURNCMD | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
                                    pt.x,
                                    pt.y,
                                    0,
                                    hwnd,
                                    None,
                                );
                                let _ = DestroyMenu(hmenu);

                                match cmd.0 {
                                    101 => {
                                        let _ = action_tx.send(TrayAction::PlayPause);
                                    }
                                    102 => {
                                        let _ = action_tx.send(TrayAction::Next);
                                    }
                                    103 => {
                                        let _ = action_tx.send(TrayAction::Previous);
                                    }
                                    104 => {
                                        let _ = action_tx.send(TrayAction::RestoreWindow);
                                        if let Some(main_raw) = main_hwnd_raw {
                                            let main_h = HWND(main_raw);
                                            ShowWindow(main_h, SW_RESTORE);
                                            SetForegroundWindow(main_h);
                                        }
                                    }
                                    105 => {
                                        let _ = action_tx.send(TrayAction::Exit);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }

        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn tray_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
