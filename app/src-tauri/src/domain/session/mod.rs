//! 会话域：Session、Message（充血：append_message / should_compact / compact）。

pub mod message;
pub mod repository;
#[allow(clippy::module_inception)]
pub mod session;

pub use message::{Message, MessageKind};
pub use repository::{MessageRepository, RecycleBinRepository, SessionRepository};
pub use session::{Session, COMPACT_THRESHOLD};
