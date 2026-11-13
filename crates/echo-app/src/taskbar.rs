#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED};

pub struct TaskbarProgress {
    #[cfg(target_os = "windows")]
    taskbar: Option<ITaskbarList3>,
    #[cfg(target_os = "windows")]
    hwnd: isize,
}

impl TaskbarProgress {
    pub fn new(_hwnd: Option<isize>) -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            let hwnd_raw = _hwnd?;
            unsafe {
                let taskbar: ITaskbarList3 =
                    CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
                let _ = taskbar.HrInit();
                Some(Self {
                    taskbar: Some(taskbar),
                    hwnd: hwnd_raw,
                })
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            None
        }
    }

    pub fn update(&mut self, is_playing: bool, is_paused: bool, position_ms: u64, duration_ms: u64) {
        #[cfg(target_os = "windows")]
        {
            let Some(ref tb) = self.taskbar else { return };
            let hwnd = HWND(self.hwnd as _);
            unsafe {
                if is_playing && duration_ms > 0 {
                    let _ = tb.SetProgressState(hwnd, TBPF_NORMAL);
                    let _ = tb.SetProgressValue(hwnd, position_ms, duration_ms);
                } else if is_paused && duration_ms > 0 {
                    let _ = tb.SetProgressState(hwnd, TBPF_PAUSED);
                    let _ = tb.SetProgressValue(hwnd, position_ms, duration_ms);
                } else {
                    let _ = tb.SetProgressState(hwnd, TBPF_NOPROGRESS);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (is_playing, is_paused, position_ms, duration_ms);
        }
    }
}

