import type { APIRoute } from 'astro';
import { allPublicPosts, xml } from '../lib/posts';
import { allPublicPages } from '../lib/pages';
export const GET: APIRoute = async ({ site, url, locals }) => {
  try {
    const base = new URL(locals.publicSiteUrl ?? site?.href ?? url.origin);
    const [posts, pages] = await Promise.all([allPublicPosts(), allPublicPages()]);
    const items = [...posts.map(post => ({ ...post, path: `/posts/${encodeURIComponent(post.slug)}` })), ...pages.map(page => ({ ...page, path: `/${encodeURIComponent(page.slug)}` }))].map(item => `<url><loc>${xml(new URL(item.path, base).href)}</loc><lastmod>${item.updated_at}</lastmod></url>`).join('');
    return new Response(`<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>${xml(base.href)}</loc></url>${items}</urlset>`, { headers: { 'content-type': 'application/xml; charset=utf-8', 'cache-control': 'no-cache' } });
  } catch { return new Response('服务暂时不可用', { status: 503, headers: { 'cache-control': 'no-store' } }); }
};
