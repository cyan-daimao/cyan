import { invoke } from '@tauri-apps/api/core';

/**
 * invoke 统一封装（TECH_DESIGN 2.5）：
 * - 错误归一为 ServiceError（code 0 成功 / 1xxx 字段级 / 2xxx 业务 / 3xxx 外部依赖 / 9001 未授权）
 * - 3xxx 可重试错误指数退避重试 3 次后再抛
 */
export class ServiceError extends Error {
  readonly code: number;

  constructor(code: number, message: string) {
    super(message);
    this.name = 'ServiceError';
    this.code = code;
  }

  /** 字段级校验错（前端红字展示） */
  get isFieldError(): boolean {
    return this.code >= 1000 && this.code < 2000;
  }
}

/** 外部依赖错误（3xxx）最大重试次数 */
const RETRY_TIMES = 3;
/** 退避基数（ms）：400 → 800 → 1600 */
const RETRY_BASE_MS = 400;

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function normalize(raw: unknown): ServiceError {
  if (raw instanceof ServiceError) return raw;
  if (typeof raw === 'object' && raw !== null) {
    const o = raw as Record<string, unknown>;
    if (typeof o.code === 'number') {
      return new ServiceError(o.code, typeof o.message === 'string' ? o.message : '未知错误');
    }
    if (typeof o.message === 'string') return new ServiceError(2000, o.message);
  }
  if (typeof raw === 'string') return new ServiceError(2000, raw);
  return new ServiceError(2000, '未知错误');
}

function isRetryable(err: ServiceError): boolean {
  return err.code >= 3000 && err.code < 4000;
}

/** 类型化 Tauri command 调用 */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  let attempt = 0;
  for (;;) {
    try {
      return await invoke<T>(cmd, args);
    } catch (raw) {
      const err = normalize(raw);
      if (isRetryable(err) && attempt < RETRY_TIMES) {
        attempt += 1;
        await sleep(RETRY_BASE_MS * 2 ** (attempt - 1));
        continue;
      }
      throw err;
    }
  }
}
