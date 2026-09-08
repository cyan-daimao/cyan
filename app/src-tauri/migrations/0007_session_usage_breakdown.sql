-- 会话 token 统计口径拆分：当前上下文大小 + 累计缓存命中
-- last_input:    最近一次 LLM 调用的 prompt token 数（即当前上下文实际大小；input_tokens 是历史累计消耗，两者语义不同）
-- cached_tokens: 累计缓存命中 token（provider 支持上下文缓存时返回；缓存部分计费低得多，单独统计便于估算真实成本）

ALTER TABLE cyan_session ADD COLUMN last_input INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cyan_session ADD COLUMN cached_tokens INTEGER NOT NULL DEFAULT 0;
