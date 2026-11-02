//! 单实例 + 聚焦信号（Windows）。
//!
//! 用**命名互斥体**而不是锁文件：进程一死，句柄由系统收走，不存在"假的人在
//! 用"，也就不会把用户挡在自己的数据外面（同类桌面应用的结论）。
//!
//! 第二个实例不再开窗：拉一下命名事件，让已有窗口自己跳到前台，然后退出。

#[cfg(windows)]
mod imp {
    use tauri::Manager;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
        INFINITE,
    };

    pub const SINGLE: &str = "Local\\com.dsh.comfyshell.single";
    pub const FOCUS: &str = "Local\\com.dsh.comfyshell.focus";

    pub struct Guard {
        handle: isize,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.handle as *mut _) };
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn acquire(name: &str) -> Result<Option<Guard>, String> {
        let n = wide(name);
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, n.as_ptr()) };
        if h.is_null() {
            return Err(format!("创建单实例互斥体失败（错误码 {}）", unsafe {
                GetLastError()
            }));
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(h) };
            return Ok(None);
        }
        Ok(Some(Guard {
            handle: h as isize,
        }))
    }

    pub fn create_event(name: &str) -> isize {
        let n = wide(name);
        unsafe { CreateEventW(std::ptr::null(), 0, 0, n.as_ptr()) as isize }
    }

    pub fn signal_event(name: &str) {
        let n = wide(name);
        let h = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, n.as_ptr()) };
        if !h.is_null() {
            unsafe {
                SetEvent(h);
                CloseHandle(h);
            }
        }
    }

    pub fn wait_event(handle: isize, ms: u32) -> bool {
        handle != 0 && unsafe { WaitForSingleObject(handle as *mut _, ms) == WAIT_OBJECT_0 }
    }

    pub fn spawn_focus_listener(event: isize, app: tauri::AppHandle) {
        if event == 0 {
            return;
        }
        std::thread::spawn(move || loop {
            if !wait_event(event, INFINITE) {
                continue;
            }
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(w) = a.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            });
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn test_name(tag: &str) -> String {
            format!("Local\\comfy-shell-test-{tag}-{}", std::process::id())
        }

        #[test]
        fn second_acquire_is_rejected_until_dropped() {
            let name = test_name("single");
            let first = acquire(&name).unwrap().expect("第一个应拿到");
            assert!(acquire(&name).unwrap().is_none(), "第二个应为 None");
            drop(first);
            assert!(acquire(&name).unwrap().is_some(), "释放后应能再拿");
        }

        #[test]
        fn focus_event_can_be_signalled_and_waited() {
            let name = test_name("evt");
            let ev = create_event(&name);
            assert_ne!(ev, 0);
            signal_event(&name);
            assert!(wait_event(ev, 1000), "事件应被唤醒");
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub const SINGLE: &str = "";
    pub const FOCUS: &str = "";
    pub struct Guard;
    pub fn acquire(_name: &str) -> Result<Option<Guard>, String> {
        Ok(Some(Guard))
    }
    pub fn create_event(_name: &str) -> isize {
        0
    }
    pub fn signal_event(_name: &str) {}
    pub fn wait_event(_handle: isize, _ms: u32) -> bool {
        false
    }
    pub fn spawn_focus_listener(_event: isize, _app: tauri::AppHandle) {}
}

pub use imp::*;

pub fn acquire_single() -> Result<Option<Guard>, String> {
    acquire(SINGLE)
}

pub fn create_focus_event() -> isize {
    create_event(FOCUS)
}

pub fn signal_focus() {
    signal_event(FOCUS)
}
