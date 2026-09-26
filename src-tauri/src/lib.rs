//! BILIdown 桌面端：把 `bili-core` 的能力通过命令暴露给界面。

mod commands;
mod state;
mod types;

use tauri::{Theme, WebviewUrl, WebviewWindowBuilder};

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
            let window = WebviewWindowBuilder::new(app.handle(), "main", WebviewUrl::default())
                .title("BILIdown")
                .inner_size(1100.0, 740.0)
                .min_inner_size(900.0, 600.0)
                .resizable(true)
                .center()
                .decorations(false)
                .initialization_script(format!(
                    "document.documentElement.dataset.theme = '{init_theme}';"
                ))
                .build()?;

            // 窗口原生主题跟随系统。WebView2 的 prefers-color-scheme 并不总是
            // 可靠，因此「跟随系统」统一以 Tauri 的窗口主题为准。
            let resolved_dark = match theme_mode.as_str() {
                "dark" => true,
                "light" => false,
                _ => matches!(window.theme(), Ok(Theme::Dark)),
            };

            // 窗口原生底色同步设置，避免 WebView2 白底闪现。
            let (r, g, b) = if resolved_dark {
                (27, 29, 33)
            } else {
                (245, 243, 244)
            };
            let _ = window.set_background_color(Some(tauri::window::Color(r, g, b, 255)));

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
        ])
        .run(tauri::generate_context!())
        .expect("BILIdown 启动失败");
}
