//! SQLite 数据访问：datasource（~/.cyan/cyan.db 连接池）+ 各业务 repo。

pub mod checkpoint_repo;
pub mod datasource;
pub mod mcp_repo;
pub mod model_repo;
pub mod perm_rule_repo;
pub mod plugin_repo;
pub mod project_repo;
pub mod recycle;
pub mod session_repo;

use chrono::{NaiveDateTime, Timelike};

/// 统一时间格式（TECH_DESIGN 第 7 章：YYYY-MM-DD HH:MM:SS）
pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// 当前本地时间（秒精度）
pub fn now_local() -> NaiveDateTime {
    let now = chrono::Local::now().naive_local();
    now.with_nanosecond(0).unwrap_or(now)
}

/// 格式化时间为存储字符串
pub fn fmt_time(t: &NaiveDateTime) -> String {
    t.format(TIME_FORMAT).to_string()
}

/// 解析存储时间字符串
pub fn parse_time(s: &str) -> anyhow::Result<NaiveDateTime> {
    Ok(NaiveDateTime::parse_from_str(s, TIME_FORMAT)?)
}

/// 解析可空时间字符串
pub fn parse_time_opt(s: &Option<String>) -> anyhow::Result<Option<NaiveDateTime>> {
    match s {
        Some(v) => Ok(Some(parse_time(v)?)),
        None => Ok(None),
    }
}
