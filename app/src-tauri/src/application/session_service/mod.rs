//! SessionService：会话与消息编排。

mod bo;
mod cmd;
mod query;
mod service;

pub use bo::{MessageBO, MessagePageBO, ProjectTokenUsageBO, SessionBO, SessionSummaryBO};
pub use cmd::{
    AppendMessageCmd, ClearSessionCmd, CreateSessionCmd, DeleteSessionCmd, EditMessageCmd,
    RenameSessionCmd, RestoreSessionCmd, SetSessionModelCmd,
};
pub use query::{GetSessionQuery, ListMessagesQuery, ListSessionQuery, ProjectTokenUsageQuery};
pub use service::{SessionService, SessionServiceImpl};
