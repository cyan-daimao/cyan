-- 会话级模型偏好：NULL = 未设置（跟随全局默认模型）

ALTER TABLE cyan_session ADD COLUMN preferred_model TEXT;
