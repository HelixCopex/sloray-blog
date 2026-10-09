import type { APIRoute } from 'astro';
import { allPublicPosts, xml } from '../lib/posts';
export const GET: APIRoute = async ({ site, url, locals }) => {
  try {
    const base = new URL(locals.publicSiteUrl ?? site?.href ?? url.origin);
    const posts = await allPublicPosts();
    const items = posts.map(post => `<item><title>${xml(post.title)}</title><link>${xml(new URL(`/posts/${encodeURIComponent(post.slug)}`, base).href)}</link><guid isPermaLink="false">${post.id}</guid><description>${xml(post.summary)}</description><pubDate>${new Date(post.published_at!).toUTCString()}</pubDate></item>`).join('');
    return new Response(`<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title>${xml(locals.siteIdentity?.site_name ?? "静线数据库")}</title><link>${xml(base.href)}</link><description>最新文章</description>${items}</channel></rss>`, { headers: { 'content-type': 'application/rss+xml; charset=utf-8', 'cache-control': 'no-cache' } });
  } catch { return new Response('服务暂时不可用', { status: 503, headers: { 'cache-control': 'no-store' } }); }
};
