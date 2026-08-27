//! adapter 层：Tauri command 入口（Controller 角色）+ Request/DTO + 事件定义。
//! 只做 Request→Cmd、调 application service、BO→DTO；不碰 Repository/DO/SQLx。

pub mod command;
pub mod dto;
pub mod event;
