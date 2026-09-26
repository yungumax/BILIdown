// 发布版不额外弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 单实例保护（OS 级命名互斥体，先于一切窗口/插件逻辑执行）。
///
/// 双击 exe 时 Windows 会背靠背发起两次启动请求，仅靠单实例插件的
/// socket 检测存在毫秒级竞态窗口，偶发第二个窗口闪现后消失——用户
/// 看到的"弹窗 + 主题自动切换"正是它。命名互斥体由内核保证互斥：
/// 第二个进程在创建任何资源之前即退出，只聚焦已有窗口。
#[cfg(windows)]
fn enforce_single_instance() {
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            lp_attributes: *const u8,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> isize;
        fn GetLastError() -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
        fn SetForegroundWindow(h_wnd: isize) -> i32;
        fn ShowWindow(h_wnd: isize, n_cmd_show: i32) -> i32;
    }

    const ERROR_ALREADY_EXISTS: i32 = 183;
    const SW_RESTORE: i32 = 9;

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let mutex_name = to_wide("BILIdown_SingleInstance_Mutex");

    // GetLastError 必须紧跟 CreateMutexW 读取，中间不能插入其他调用
    let (handle, already_running) = unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        let err = GetLastError();
        (handle, err == ERROR_ALREADY_EXISTS)
    };

    if already_running {
        // 已有实例在运行：还原并聚焦其窗口，然后本次启动直接退出。
        // 互斥体句柄有意不关闭——进程退出时内核自动回收并释放锁。
        unsafe {
            let window_title = to_wide("BILIdown");
            let hwnd = FindWindowW(std::ptr::null(), window_title.as_ptr());
            if hwnd != 0 {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
        std::process::exit(0);
    }

    // 主实例：句柄保持存活直到进程退出（isize 原始句柄不会自动关闭，
    // 内核在进程结束时回收互斥体并释放锁）
    let _keep_mutex_alive = handle;
}

#[cfg(not(windows))]
fn enforce_single_instance() {}

fn main() {
    enforce_single_instance();

    bilidown_lib::run()
}
