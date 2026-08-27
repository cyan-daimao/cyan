//! ModelConfig：模型配置充血对象（normalize / validate / mask_key）。

use chrono::NaiveDateTime;

use crate::domain::DomainError;

/// 模型启用状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelStatus {
    /// 启用
    Enabled,
    /// 禁用
    Disabled,
}

impl ModelStatus {
    /// 存储字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }

    /// 从存储字符串解析
    pub fn parse(s: &str) -> Self {
        match s {
            "disabled" => Self::Disabled,
            _ => Self::Enabled,
        }
    }
}

/// 模型配置
#[derive(Debug, Clone)]
pub struct ModelConfig {
    /// 主键 id（插入后回填）
    pub id: i64,
    /// 模型名（唯一）
    pub name: String,
    /// Provider
    pub provider: String,
    /// Base URL（normalize 后无尾斜杠）
    pub base_url: String,
    /// API Key 引用串（真实值存 OS keychain，库内仅存 `keychain://cyan/model/<name>`）
    pub api_key_ref: String,
    /// 上下文窗口（token 数）
    pub context_window: i64,
    /// 是否默认模型（应用层保证唯一）
    pub is_default: bool,
    /// 启用状态
    pub status: ModelStatus,
    /// 创建时间
    pub created_at: NaiveDateTime,
    /// 更新时间
    pub updated_at: NaiveDateTime,
}

impl ModelConfig {
    /// 新建（未持久化，id 待回填）
    pub fn new(
        name: String,
        provider: String,
        base_url: String,
        context_window: i64,
        now: NaiveDateTime,
    ) -> Self {
        Self {
            id: 0,
            name,
            provider,
            base_url,
            api_key_ref: String::new(),
            context_window,
            is_default: false,
            status: ModelStatus::Enabled,
            created_at: now,
            updated_at: now,
        }
    }

    /// keychain 引用串
    pub fn keychain_ref(name: &str) -> String {
        format!("keychain://cyan/model/{name}")
    }

    /// 规范化：baseUrl 去尾斜杠、名称去空白
    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.provider = self.provider.trim().to_string();
        self.base_url = self.base_url.trim().trim_end_matches('/').to_string();
    }

    /// 校验（PRD 7.1）：名称非空 ≤50、provider 非空、baseUrl 为 http(s)、上下文窗口 ≥ 1000
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.name.is_empty() {
            return Err(DomainError::Validation("模型名不能为空".into()));
        }
        if self.name.chars().count() > 50 {
            return Err(DomainError::Validation("模型名不能超过 50 字符".into()));
        }
        if self.provider.is_empty() {
            return Err(DomainError::Validation("Provider 不能为空".into()));
        }
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err(DomainError::Validation(
                "Base URL 必须以 http:// 或 https:// 开头".into(),
            ));
        }
        if self.context_window < 1000 {
            return Err(DomainError::Validation(
                "上下文窗口不能小于 1000".into(),
            ));
        }
        Ok(())
    }

    /// API Key 脱敏：`sk-****xxxx`（保留末 4 位），长度 ≤4 时全掩码
    pub fn mask_key(key: &str) -> String {
        let chars: Vec<char> = key.chars().collect();
        if chars.len() <= 4 {
            return "****".to_string();
        }
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("sk-****{tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_model() -> ModelConfig {
        let now = NaiveDateTime::default();
        ModelConfig::new(
            "kimi".into(),
            "moonshot".into(),
            "https://api.moonshot.cn/v1".into(),
            128_000,
            now,
        )
    }

    #[test]
    fn validate_ok() {
        let mut m = base_model();
        m.normalize();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_name() {
        let mut m = base_model();
        m.name = "  ".into();
        m.normalize();
        assert!(matches!(m.validate(), Err(DomainError::Validation(_))));
    }

    #[test]
    fn validate_rejects_bad_base_url() {
        let mut m = base_model();
        m.base_url = "api.moonshot.cn".into();
        assert!(matches!(m.validate(), Err(DomainError::Validation(_))));
    }

    #[test]
    fn validate_rejects_small_context_window() {
        let mut m = base_model();
        m.context_window = 512;
        assert!(matches!(m.validate(), Err(DomainError::Validation(_))));
    }

    #[test]
    fn normalize_strips_trailing_slash() {
        let mut m = base_model();
        m.base_url = "https://api.moonshot.cn/v1/".into();
        m.normalize();
        assert_eq!(m.base_url, "https://api.moonshot.cn/v1");
    }

    #[test]
    fn mask_key_keeps_last4() {
        assert_eq!(ModelConfig::mask_key("sk-abcdef123456"), "sk-****3456");
        assert_eq!(ModelConfig::mask_key("abcd"), "****");
        assert_eq!(ModelConfig::mask_key(""), "****");
    }
}
