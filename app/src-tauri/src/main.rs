// cyan 桌面入口：完整 Builder 在 lib.rs（命令注册、状态注入、日志、数据库初始化）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cyan_lib::run();
}
