//! BILIdown 桌面端：把 `bili-core` 的能力通过命令暴露给界面。

mod commands;
mod state;
mod types;

pub fn run() {
    let state = state::AppState::new().expect("初始化应用状态失败");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::probe_video,
            commands::start_download,
            commands::cancel_download,
            commands::login_qrcode,
            commands::login_poll,
            commands::logout,
            commands::choose_output_dir,
            commands::set_output_dir,
            commands::open_path,
        ])
        .run(tauri::generate_context!())
        .expect("BILIdown 启动失败");
}
