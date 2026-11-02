// 发布版不弹控制台窗口（Windows）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--print-command") {
        let version = args.get(i + 1).cloned().unwrap_or_default();
        if version.is_empty() {
            eprintln!("用法：glassmc --print-command <版本 id>");
            std::process::exit(2);
        }
        match glassmc_lib::describe_command(&version) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--install") {
        let version = args.get(i + 1).cloned().unwrap_or_default();
        if version.is_empty() {
            eprintln!("用法：glassmc --install <版本 id>");
            std::process::exit(2);
        }
        match glassmc_lib::cli_install(&version) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--test-terracotta") {
        match glassmc_lib::cli_test_terracotta() {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("失败：{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    glassmc_lib::run()
}
