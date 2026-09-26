//! BILIdown 桌面端：把 `bili-core` 的能力通过命令暴露给界面。

mod commands;
mod state;
mod types;

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// 主题模式对应的窗口原生底色（与 CSS 令牌一致）。
fn theme_bg(mode: &str) -> tauri::window::Color {
    let dark = match mode {
        "dark" => true,
        "light" => false,
        // 跟随系统：以构建时系统偏好为准（页面本身由 CSS 精确控制）
        _ => true, // Windows 深色更常见于深色模式用户；media query 才是精确来源
    };
    if dark {
        tauri::window::Color(27, 29, 33, 255)
    } else {
        tauri::window::Color(245, 243, 244, 255)
    }
}

pub fn run() {
    let state = state::AppState::new().expect("初始化应用状态失败");
    // 启动前读出主题，注入初始化脚本：页面首帧即为正确配色，杜绝启动闪白
    let theme_mode = state.settings().theme.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            // 初始化脚本在页面任何脚本执行前运行：跟随系统时直接写 "system"，
            // 由 CSS 的 prefers-color-scheme media query 在首帧完成配色。
            let init_theme = match theme_mode.as_str() {
                "dark" => "dark",
                "light" => "light",
                _ => "system",
            };
            // 窗口先隐藏，前端渲染完成后由前端调 show_window 显示，
            // 彻底避免「先见白底/旧底色、再见内容」的启动闪烁。
            let _window = WebviewWindowBuilder::new(app.handle(), "main", WebviewUrl::default())
                .title("BILIdown")
                .inner_size(1100.0, 740.0)
                .min_inner_size(900.0, 600.0)
                .resizable(true)
                .center()
                .decorations(false)
                .visible(false)
                .background_color(theme_bg(&theme_mode))
                .initialization_script(format!(
                    "document.documentElement.dataset.theme = '{init_theme}';"
                ))
                .build()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::app_settings,
            commands::update_settings,
            commands::probe_source,
            commands::start_download,
            commands::cancel_download,
            commands::login_qrcode,
            commands::login_poll,
            commands::logout,
            commands::choose_output_dir,
            commands::set_output_dir,
            commands::open_path,
            commands::pick_ffmpeg,
            commands::cleanup_temp,
            commands::cleanup_cache,
            commands::export_diagnostics,
            commands::check_updates,
            commands::show_window,
        ])
        .run(tauri::generate_context!())
        .expect("BILIdown 启动失败");
}
