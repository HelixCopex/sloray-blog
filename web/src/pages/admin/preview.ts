import type { APIRoute } from 'astro';
import { serverGet, type Session } from '../../lib/api';
import { renderMarkdown } from '../../lib/render-markdown';

export const POST: APIRoute = async ({ request }) => {
  const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json', 'cache-control': 'no-store' } });
  try {
    const session = await serverGet<Session>('/auth/session', { headers: { cookie: request.headers.get('cookie') ?? '' } });
    if (!session.user) return json({ error: { message: '请先登录。' } }, 401);
    if (request.headers.get('x-csrf-token') !== session.csrf_token) return json({ error: { message: '请刷新页面后重试。' } }, 403);
    if (!request.headers.get('content-type')?.startsWith('application/json')) return json({ error: { message: '需要 JSON 正文。' } }, 415);
    const reader = request.body?.getReader();
    if (!reader) return json({ error: { message: '缺少正文。' } }, 400);
    const chunks: Uint8Array[] = [];
    let length = 0;
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      length += chunk.value.byteLength;
      if (length > 2_100_000) { await reader.cancel(); return json({ error: { message: '正文过长。' } }, 413); }
      chunks.push(chunk.value);
    }
    const bytes = Buffer.concat(chunks);
    let input: { body_md?: unknown };
    try { input = JSON.parse(bytes.toString('utf8')); } catch { return json({ error: { message: '无效的 JSON。' } }, 400); }
    if (!input || typeof input.body_md !== 'string' || Buffer.byteLength(input.body_md) > 512_000) return json({ error: { message: '正文无效或过长。' } }, 422);
    return json({ html: await renderMarkdown(input.body_md) });
  } catch { return json({ error: { message: '预览暂时不可用，请重试。' } }, 503); }
};
