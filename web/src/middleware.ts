import { defineMiddleware } from 'astro:middleware';
import { serverGet } from './lib/api';
import { createHash } from 'node:crypto';
// HTML 与 JSON 是不同表示，各自计算 ETag；模板或资源变更也必须使 HTML 失效。
export const onRequest = defineMiddleware(async (context, next) => {
  if (!context.isPrerendered) {
    try { context.locals.publicSiteUrl = (await serverGet<{ public_site_url: string | null }>('/site')).public_site_url ?? undefined; } catch { /* 未配置或暂不可达时使用构建地址。 */ }
  }
  const response = await next();
  // 动态导航包含登录后的管理入口；共享缓存必须按 Cookie 区分 HTML。
  if (!context.isPrerendered) response.headers.append('vary', 'Cookie');
  if (context.url.pathname.startsWith('/posts/') && response.status === 200) {
    const html = await response.text();
    const etag = `"${createHash('sha256').update(html).digest('hex')}"`;
    const headers = new Headers(response.headers);
    headers.set('etag', etag);
    headers.set('cache-control', 'public, no-cache');
    const matches = context.request.headers.get('if-none-match')?.split(',').some(v => v.trim().replace(/^W\//, '') === etag || v.trim() === '*');
    if (matches) { headers.delete('content-length'); return new Response(null, { status: 304, headers }); }
    return new Response(html, { status: 200, headers });
  }
  if (!response.headers.has('cache-control')) response.headers.set('cache-control', 'no-store');
  return response;
});
