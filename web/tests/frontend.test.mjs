import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import ts from 'typescript';
import { defaultTheme, themeCss, previewHtml } from '../src/lib/theme.mjs';
import { mediaMarkdown } from '../src/lib/media-markdown.mjs';
const require = createRequire(import.meta.url);
const codeUrl = code => 'data:text/javascript;base64,' + Buffer.from(code).toString('base64');
// Compile using the project's existing TypeScript version; works on Node 22.12 without experimental loaders.
async function compile(name, replacements = {}) {
  let source = await fs.readFile(new URL(`../src/lib/${name}.ts`, import.meta.url), 'utf8');
  for (const [from,to] of Object.entries(replacements)) source = source.replaceAll(from,to);
  return codeUrl(ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText);
}
const apiUrl = await compile('api', {'import.meta.env.SLORAY_API_INTERNAL_URL':'undefined'});
const api = await import(apiUrl);
const headingsUrl = await compile('markdown');
const markdown = await import(await compile('render-markdown', {
  "'./markdown'": JSON.stringify(headingsUrl),
  "'@astrojs/markdown-remark'": JSON.stringify(pathToFileURL(require.resolve('@astrojs/markdown-remark')).href),
  "'rehype-sanitize'": JSON.stringify(pathToFileURL(require.resolve('rehype-sanitize')).href),
}));
const upload = await import(await compile('media-upload', {"'./api'": JSON.stringify(apiUrl)}));

test('Markdown renders GFM, code and images while stripping executable HTML and URLs', async () => {
  const html = await markdown.renderMarkdown('# 标题\n\n**粗体**\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```js\nconst x = 1\n```\n\n![示例](https://example.com/image.webp)\n\n<script>alert(1)</script>\n\n[坏链接](javascript:alert(1))\n\n<img src=x onerror=alert(1)>');
  assert.match(html, /<h2/); assert.doesNotMatch(html, /<h1|<script|onerror=|href="javascript:/);
  assert.match(html, /<strong>粗体<\/strong>/); assert.match(html, /<table>/); assert.match(html, /<pre/); assert.match(html, /alt="示例"/);
});
test('Markdown headings saturate at h6 and plain text remains escaped', async () => {
  const html = await markdown.renderMarkdown('###### 最深层\n\n&lt;script&gt;文字&lt;/script&gt;');
  assert.match(html, /<h6/); assert.doesNotMatch(html, /<h7|<script/);
});
test('media clipboard links escape labels, multiline text and unsafe destination punctuation', async () => {
  const link = mediaMarkdown(' 图[片]\\\n换行 ', 'https://example.com/a b(1)<x>.webp');
  assert.equal(link, '![图\\[片\\]\\\\ 换行](<https://example.com/a%20b(1)%3Cx%3E.webp>)');
  assert.match(await markdown.renderMarkdown(link), /alt="图\[片\]\\ 换行"/);
  assert.equal(mediaMarkdown(' ', 'https://example.com/a.webp'), '![图片](<https://example.com/a.webp>)');
});
test('upload diagnostics omit signatures, credentials and raw unknown failures', () => {
  const result = upload.describeUploadFailure(new Error('secret-token'), {stage:'原图',origin:'https://blog.example',contentType:'image/png',sizeBytes:10,uploadUrl:'https://objects.example/key?X-Amz-Credential=SECRET&X-Amz-Signature=PRIVATE',headers:{Authorization:'PRIVATE'}});
  assert.match(result.details, /https:\/\/objects.example/);
  assert.doesNotMatch(JSON.stringify(result), /SECRET|PRIVATE|secret-token|X-Amz-Signature=/);
});
test('theme preview uses the chosen block order and escapes all HTML text', () => {
  const p = structuredClone(defaultTheme); p.layout.home_blocks = ['posts','hero','pages']; p.layout.hero_title = '<img src=x onerror=alert(1)>';
  const html = previewHtml(p, 'body{}');
  assert.ok(html.indexOf('文章</h2>') < html.indexOf('class="hero"'));
  assert.match(html, /&lt;img/); assert.doesNotMatch(html, /<script|<img src=x/); assert.match(html, /Content-Security-Policy/);
});
test('CSS cannot terminate style elements and theme dimensions stay bounded', () => {
  const p = structuredClone(defaultTheme); p.css = '</style><script>alert(1)</script>'; p.layout.width = Infinity; p.layout.columns = 100;
  const css = themeCss(p); assert.doesNotMatch(css, /</); assert.match(css, /--content-width:1600px/); assert.match(css, /--post-columns:3/);
});
test('client API returns typed errors, sends CSRF with credentials and handles 204', async t => {
  const calls = []; const original = globalThis.fetch;
  t.after(() => { globalThis.fetch = original; });
  globalThis.fetch = async (url, init) => { calls.push({url,init}); return url.endsWith('/auth/session') ? Response.json({csrf_token:'token'}) : new Response(null,{status:204}); };
  assert.equal(await api.clientWrite('/admin/themes/id','DELETE',{version:1}), undefined);
  assert.equal(calls[1].init.headers.get('x-csrf-token'),'token'); assert.equal(calls[1].init.credentials,'same-origin');
  globalThis.fetch = async () => Response.json({error:{code:'conflict',message:'changed'}},{status:409});
  await assert.rejects(api.clientGet('/x'), e => e instanceof api.ApiRequestError && e.status === 409 && e.code === 'conflict');
  globalThis.fetch = async () => new Response('<h1>Bad gateway</h1>',{status:502});
  await assert.rejects(api.clientGet('/x'), e => e.status === 502 && !e.message.includes('<h1>'));
});
test('SSR retries unavailable upstreams, but preserves authorization and abort semantics', async t => {
  const original = globalThis.fetch, previous = process.env.SLORAY_API_INTERNAL_URLS;
  t.after(() => { globalThis.fetch = original; if (previous === undefined) delete process.env.SLORAY_API_INTERNAL_URLS; else process.env.SLORAY_API_INTERNAL_URLS = previous; });
  process.env.SLORAY_API_INTERNAL_URLS = 'http://a,http://b';
  const urls = []; globalThis.fetch = async url => { urls.push(url); return url.startsWith('http://a') ? new Response('',{status:503}) : Response.json({ok:true}); };
  assert.deepEqual(await api.serverGet('/site'),{ok:true}); assert.equal(urls.length,2);
  globalThis.fetch = async () => Response.json({error:{code:'forbidden',message:'forbidden'}},{status:403});
  await assert.rejects(api.serverGet('/admin/themes'), e => e.status===403);
  await assert.rejects(api.serverGet('/site',{method:'POST'}), /只支持读取/);
  const controller = new AbortController(); controller.abort();
  globalThis.fetch = async () => { throw controller.signal.reason; };
  await assert.rejects(api.serverGet('/site',{signal:controller.signal}), {name:'AbortError'});
});
