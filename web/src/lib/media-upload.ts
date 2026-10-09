/** 原图直传与可公开展示的失败诊断；不包含签名 URL、响应原文或请求头值。 */
import { ApiRequestError } from './api';

export interface UploadContext {
  stage: string;
  origin: string;
  contentType: string;
  sizeBytes: number;
  uploadId?: string;
  uploadUrl?: string;
  headers?: Record<string, string>;
}

type FailureKind = 'http' | 'network' | 'timeout' | 'abort' | 'processing';

export class MediaUploadError extends Error {
  constructor(
    message: string,
    readonly kind: FailureKind,
    readonly status?: number,
    readonly storageCode?: string,
    readonly requestId?: string,
  ) { super(message); }
}

function safeIdentifier(value: string | null | undefined): string | undefined {
  return value && /^[A-Za-z0-9._-]{1,128}$/.test(value) ? value : undefined;
}

function storageFailure(request: XMLHttpRequest): MediaUploadError {
  const xml = new DOMParser().parseFromString(request.responseText, 'application/xml');
  const code = safeIdentifier(xml.getElementsByTagNameNS('*', 'Code')[0]?.textContent);
  const bodyId = safeIdentifier(xml.getElementsByTagNameNS('*', 'RequestId')[0]?.textContent);
  // 仅尝试读取已经向浏览器公开的头，避免未 expose 的头产生额外控制台错误。
  const exposedId = request.getAllResponseHeaders().match(/^x-amz-request-id:\s*(.+)$/im)?.[1]?.trim();
  const requestId = bodyId ?? safeIdentifier(exposedId);
  const hint = request.status === 403 ? '请检查存储桶写入权限、签名和上传许可是否过期。'
    : request.status === 412 ? '对象已存在，请重新点击上传取得新许可。'
    : request.status === 400 ? '请检查图片类型、校验和及存储服务兼容性。'
    : request.status >= 500 ? '存储服务暂时不可用，请稍后重试。' : '请展开错误详情排查。';
  return new MediaUploadError(`原图上传失败（HTTP ${request.status}${code ? ` / ${code}` : ''}）。${hint}`,
    'http', request.status, code, requestId);
}

export function uploadOriginal(url: string, file: File, headers: Record<string, string>,
  onProgress: (percent: number) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('PUT', url);
    for (const [name, value] of Object.entries(headers)) request.setRequestHeader(name, value);
    request.upload.onprogress = event => {
      if (event.lengthComputable) onProgress(event.loaded * 100 / event.total);
    };
    request.onload = () => request.status >= 200 && request.status < 300 ? resolve() : reject(storageFailure(request));
    request.onerror = () => reject(new MediaUploadError(
      '原图上传连接失败：浏览器未提供 HTTP 响应。请检查原图桶 CORS、网络和 TLS，展开错误详情查看本站来源与所需请求头。', 'network', 0));
    request.timeout = 120000;
    request.ontimeout = () => reject(new MediaUploadError('原图上传超时（120 秒），请检查网络后重试。', 'timeout', 0));
    request.onabort = () => reject(new MediaUploadError('原图上传已中止，可重新点击上传。', 'abort', 0));
    request.send(file);
  });
}

export function describeUploadFailure(error: unknown, context: UploadContext): { message: string; details: string } {
  let endpoint = '尚未取得上传地址';
  if (context.uploadUrl) {
    try { endpoint = new URL(context.uploadUrl).origin; }
    catch { endpoint = '上传端点无效'; }
  }
  const known = error instanceof MediaUploadError || error instanceof ApiRequestError;
  const message = error instanceof MediaUploadError ? error.message
    : error instanceof ApiRequestError ? `${context.stage}：${error.message}`
    : `${context.stage}失败，请检查网络与本站 API，展开错误详情排查。`;
  const status = known ? error.status : undefined;
  const details = [
    `失败阶段：${context.stage}`,
    `发生时间：${new Date().toISOString()}`,
    `HTTP 状态：${status === 0 ? '0（浏览器未提供可读的 HTTP 响应）' : status ?? '未取得响应'}`,
    `本站 Origin：${context.origin}`,
    `存储端点：${endpoint}`,
    `图片类型：${context.contentType || '未知'}`,
    `图片大小：${context.sizeBytes} 字节`,
  ];
  if (context.uploadId) details.push(`上传编号：${context.uploadId}`);
  if (context.headers) details.push(`PUT 所需请求头：${Object.keys(context.headers).sort().join(', ')}`);
  if (error instanceof MediaUploadError) {
    if (error.storageCode) details.push(`存储错误代码：${error.storageCode}`);
    if (error.requestId) details.push(`存储请求编号：${error.requestId}`);
    if (error.kind === 'network') details.push(
      '可能原因：CORS 预检被拒绝、DNS/网络/TLS 失败或浏览器拦截；前端无法据此确定具体原因。',
      '排查：在原图桶允许上述精确 Origin、PUT 和所需请求头；在浏览器 Network/Console 查看 OPTIONS 与 PUT 的错误。');
  }
  if (error instanceof ApiRequestError) details.push(`API 错误代码：${safeIdentifier(error.code) ?? 'unknown_error'}`);
  return { message, details: details.join('\n') };
}
