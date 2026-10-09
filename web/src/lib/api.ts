/** 同源浏览器客户端与 SSR 内网客户端。类型由 OpenAPI 生成。 */
import type { components } from './schema';
export type Post = components['schemas']['Post'];
export type PostPage = components['schemas']['PostPage'];
export type PostSummary = components['schemas']['PostSummary'];
export type Tag = components['schemas']['Tag'];
export type WritePost = components['schemas']['WritePost'];
export type Revision = components['schemas']['Revision'];
export type RevisionPage = components['schemas']['RevisionPage'];
export type Session = components['schemas']['Session'];
export type Page = components['schemas']['Page'];
export type PageList = components['schemas']['PageList'];
export type PageSummary = components['schemas']['PageSummary'];
export type WritePage = components['schemas']['WritePage'];
export const API_PREFIX = '/api/v1';
export interface Pong { pong: boolean; version: string }
export interface ApiErrorBody { error: { code: string; message: string } }

export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
  }
}

async function parse<T>(response: Response): Promise<T> {
  const text = await response.text();
  if (!response.ok) {
    let code = 'unknown_error';
    let message = `请求失败（HTTP ${response.status}）`;
    try {
      const body = JSON.parse(text) as ApiErrorBody;
      if (body.error?.code) { code = body.error.code; message = body.error.message; }
    } catch { /* 代理可能返回 HTML 错误页。 */ }
    throw new ApiRequestError(response.status, code, message);
  }
  return text ? JSON.parse(text) as T : undefined as T;
}

export async function serverGet<T>(path: string, init?: RequestInit): Promise<T> {
  if (init?.method && !['GET', 'HEAD'].includes(init.method)) throw new Error('serverGet 只支持读取');
  const configured = process.env.SLORAY_API_INTERNAL_URLS ?? import.meta.env.SLORAY_API_INTERNAL_URL;
  const bases = configured ? configured.split(',').map((s: string) => s.trim().replace(/\/$/, '')) : ['http://127.0.0.1:8080', 'http://127.0.0.1:8081'];
  let last: unknown;
  for (const base of bases) {
    try {
      const headers = new Headers(init?.headers);
      headers.set('accept', 'application/json');
      const response = await fetch(`${base}${API_PREFIX}${path}`, { ...init, headers, signal: init?.signal ?? AbortSignal.timeout(5000) });
      if ([502, 503, 504].includes(response.status)) { last = new ApiRequestError(response.status, 'unavailable', '服务暂时不可用'); continue; }
      return await parse<T>(response);
    } catch (error) {
      if (error instanceof ApiRequestError) throw error;
      last = error;
      if (init?.signal?.aborted) throw error;
    }
  }
  throw last ?? new Error('API 不可达');
}

export async function clientGet<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set('accept', 'application/json');
  return parse<T>(await fetch(`${API_PREFIX}${path}`, { ...init, credentials: 'same-origin', headers }));
}

export async function clientWrite<T>(path: string, method: 'POST' | 'PUT' | 'DELETE', body: unknown): Promise<T> {
  const session = await clientGet<Session>('/auth/session');
  return clientGet<T>(path, { method, headers: { 'content-type': 'application/json', 'x-csrf-token': session.csrf_token }, body: JSON.stringify(body) });
}
