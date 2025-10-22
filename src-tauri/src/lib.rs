mod commands;
mod sync;
mod tunnel;
mod webdav;

// 平台特定的 tunnel 模块
#[cfg(target_os = "macos")]
mod tunnel_macos;
#[cfg(target_os = "linux")]
mod tunnel_linux;
#[cfg(target_os = "windows")]
mod tunnel_windows;

use chrono::Local;
use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_log::{Target, TargetKind};

#[cfg(target_os = "linux")]
mod daemon;
#[cfg(target_os = "linux")]
mod daemon_install;
#[cfg(target_os = "linux")]
mod daemon_ipc;

#[cfg(target_os = "linux")]
pub use daemon::run_daemon;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_file_name = format!("{}", Local::now().format("%Y-%m-%d_%H-%M-%S"));
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some(log_file_name),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            log::info!("========== WG-X 应用启动 ==========");
            log::info!("平台: {}", std::env::consts::OS);
            log::info!("应用数据目录: {:?}", app.path().app_data_dir());
            log::info!("应用日志目录: {:?}", app.path().app_log_dir());
            log::info!("=====================================");

            let win_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("")
                .fullscreen(false)
                .resizable(false)
                .inner_size(1000.0, 810.0);

            #[cfg(target_os = "macos")]
            let win_builder = win_builder.title_bar_style(TitleBarStyle::Transparent);

            let window = win_builder.build().unwrap();

            #[cfg(target_os = "macos")]
            {
                use cocoa::appkit::{NSColor, NSWindow};
                use cocoa::base::{id, nil};

                let ns_window = window.ns_window().unwrap() as id;
                unsafe {
                    let bg_color = NSColor::colorWithRed_green_blue_alpha_(
                        nil,
                        102.0 / 255.0,
                        126.0 / 255.0,
                        234.5 / 255.0,
                        1.0,
                    );
                    ns_window.setBackgroundColor_(bg_color);
                }
            }

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::misc_commands::get_platform,
            commands::key_management::generate_keypair,
            commands::key_management::generate_preshared_key,
            commands::key_management::private_key_to_public,
            commands::env_config::load_env_config,
            commands::persistence::get_next_peer_id,
            commands::config_templates::generate_wg_config,
            commands::config_templates::generate_ikuai_config,
            commands::config_templates::generate_surge_config,
            commands::config_templates::generate_mikrotik_config,
            commands::config_templates::generate_openwrt_config,
            commands::persistence::save_persistent_config,
            commands::persistence::load_persistent_config,
            commands::misc_commands::generate_qrcode,
            commands::misc_commands::save_config_to_path,
            commands::history_service::save_to_history,
            commands::history_service::get_history_list,
            commands::history_service::get_history_detail,
            commands::history_service::delete_history,
            commands::history_service::clear_all_history,
            commands::persistence::clear_cached_config,
            commands::history_service::export_all_configs_zip,
            commands::server_service::save_server_config,
            commands::server_service::get_server_list,
            commands::server_service::get_server_detail,
            commands::server_service::delete_server,
            commands::server_service::clear_all_servers,
            commands::server_service::get_next_peer_id_for_server,
            commands::server_service::update_server_peer_id,
            commands::history_service::get_history_list_by_server,
            commands::server_service::migrate_old_config_to_server,
            commands::webdav_commands::save_webdav_config,
            commands::webdav_commands::load_webdav_config,
            commands::webdav_commands::test_webdav_connection,
            commands::webdav_commands::sync_to_webdav,
            commands::webdav_commands::sync_from_webdav,
            commands::webdav_commands::sync_bidirectional_webdav,
            commands::webdav_commands::save_last_sync_info,
            commands::webdav_commands::load_last_sync_info,
            tunnel::start_tunnel,
            tunnel::stop_tunnel,
            tunnel::get_tunnel_list,
            tunnel::get_tunnel_details,
            tunnel::save_tunnel_config,
            tunnel::delete_tunnel_config,
            tunnel::get_all_tunnel_configs,
            tunnel::get_tunnel_config,
            #[cfg(target_os = "linux")]
            daemon_install::check_daemon_status,
            #[cfg(target_os = "linux")]
            daemon_install::install_daemon,
            #[cfg(target_os = "linux")]
            daemon_install::uninstall_daemon,
            #[cfg(target_os = "linux")]
            daemon_install::start_daemon_service,
            #[cfg(target_os = "linux")]
            daemon_install::stop_daemon_service,
            #[cfg(target_os = "linux")]
            daemon_install::restart_daemon_service,
            #[cfg(target_os = "linux")]
            daemon_install::enable_daemon_service,
            #[cfg(target_os = "linux")]
            daemon_install::disable_daemon_service,
            #[cfg(target_os = "linux")]
            daemon_install::get_daemon_logs
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                log::info!("========== WG-X 应用关闭 ==========");
                log::info!("=====================================");
            }
        });
}
