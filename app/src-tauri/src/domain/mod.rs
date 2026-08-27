//! domain 层：充血领域对象与 Repository trait，不依赖 tauri/sqlx/reqwest。

pub mod agent;
pub mod config;
pub mod error;
pub mod project;
pub mod session;
pub mod shared;

pub use error::DomainError;
