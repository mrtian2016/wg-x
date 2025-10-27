// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 在 Tauri 初始化之前检查命令行参数
    // 这样可以避免在纯命令行模式下启动 GUI
    let args: Vec<String> = std::env::args().collect();

    // 检查 --version 或 -V
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return;
    }

    // 检查 --help 或 -h
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return;
    }

    // 检查 daemon 子命令 (仅 Linux)
    #[cfg(target_os = "linux")]
    if args.len() > 1 && args[1] == "daemon" {
        run_daemon_mode();
        return;
    }

    // 默认情况：启动 GUI
    wire_vault_lib::run();
}

fn print_help() {
    println!("WireVault {}", env!("CARGO_PKG_VERSION"));
    println!("WireGuard 隧道管理工具");
    println!();
    println!("用法:");
    println!("  wire-vault                  启动图形界面 (默认)");
    println!("  wire-vault [选项]");
    #[cfg(target_os = "linux")]
    println!("  wire-vault [子命令]");
    println!();
    println!("选项:");
    println!("  -h, --help            显示此帮助信息");
    println!("  -V, --version         显示版本号");
    println!();
    #[cfg(target_os = "linux")]
    {
        println!("子命令:");
        println!("  daemon                运行守护进程 (需要 root 权限)");
        println!();
    }
}

#[cfg(target_os = "linux")]
fn run_daemon_mode() {
    tokio::runtime::Runtime::new()
        .expect("无法创建 tokio runtime")
        .block_on(async {
            if let Err(e) = wire_vault_lib::run_daemon().await {
                eprintln!("守护进程错误: {}", e);
                std::process::exit(1);
            }
        });
}
