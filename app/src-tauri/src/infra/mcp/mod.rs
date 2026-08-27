//! MCP：stdio 子进程接入的 CRUD 状态机骨架。
//! 1.0.0 仅提供 CRUD 与状态流转（disabled → connecting → connected / error），
//! 不接 initialize 握手与工具注入（`mcp__<server>__<tool>` 命名预留，后续迭代接入）。

use crate::domain::config::McpServer;
use crate::domain::DomainError;

/// MCP 工具名前缀拼装（预留）：`mcp__<server>__<tool>`
pub fn tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{server}__{tool}")
}

/// 连接握手（骨架实现）：校验命令非空后直接视为连接成功、发现 0 个工具。
/// 真实 stdio initialize/tools-list 握手在后续迭代接入。
pub fn handshake(server: &mut McpServer) -> Result<(), DomainError> {
    server.connect()?;
    if server.command.trim().is_empty() {
        server.mark_error("启动命令为空".into());
        return Err(DomainError::Validation("MCP 启动命令为空".into()));
    }
    // 骨架阶段：不做真实子进程握手
    server.mark_connected(0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::McpStatus;

    #[test]
    fn handshake_skeleton_connected() {
        let mut s = McpServer::new("fs".into(), "npx mcp-fs".into(), chrono::NaiveDateTime::default());
        handshake(&mut s).unwrap();
        assert_eq!(s.status, McpStatus::Connected);
        assert_eq!(tool_name("fs", "read"), "mcp__fs__read");
    }
}
