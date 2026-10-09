/** Pure theme presentation helpers shared by SSR, preview and tests. */
export const defaultTheme = {
  schema_version: 1, name: '静线 · Material', description: '默认 Material 3 主题',
  palette: 'blue', font: 'sans', css: '',
  layout: { width: 1120, navigation: 'side', posts: 'list', columns: 1,
    home_blocks: ['hero', 'widgets', 'posts'], hero_title: '记录当下，连接想法。',
    hero_description: '写作、分享，以及值得留下的讨论。', footer: '记录、分享与讨论' },
};
const palettes = {
  violet: ['#6750a4','#eaddff','#21005d','#d0bcff','#4f378b','#381e72'],
  teal: ['#006a60','#9ef2e3','#00201c','#82d5c7','#005048','#003730'],
  blue: ['#005ac1','#d8e2ff','#001a41','#adc6ff','#004494','#002e69'],
  rose: ['#984061','#ffd9e2','#3e001d','#ffb0c8','#7b294a','#5f1134'],
};
const neutrals = {
  violet: [['#fef7ff','#f3edf7','#ece6f0','#1d1b20','#49454f','#79747e','#cac4d0'],['#141218','#211f26','#2b2930','#e6e0e9','#cac4d0','#938f99','#49454f']],
  teal: [['#f4fbf8','#e8f1ed','#e2ebe7','#161d1b','#3f4946','#707975','#bfc9c4'],['#0e1513','#1b211f','#252b29','#dee4e0','#bfc9c4','#89938e','#3f4946']],
  blue: [['#f9f9ff','#edf0f9','#e7eaf3','#191c20','#43474e','#74777f','#c3c6d0'],['#111318','#1e2025','#282a2f','#e2e2e9','#c3c6d0','#8d9199','#43474e']],
  rose: [['#fff8f8','#f9e9ed','#f3e3e7','#211a1c','#514348','#837379','#d6c2c8'],['#191114','#281c20','#32262a','#efdee1','#d6c2c8','#9f8c92','#514348']],
};
const roles = ['surface','surface-container','surface-high','on-surface','on-surface-variant','outline','outline-variant'];
const declarations = colors => roles.map((role, i) => `--${role}:${colors[i]}`).join(';');
const fonts = {
  sans: 'ui-sans-serif,system-ui,-apple-system,"Noto Sans SC",sans-serif',
  serif: 'ui-serif,"Noto Serif SC",serif', mono: 'ui-monospace,"Noto Sans Mono",monospace',
};
/** Escape the HTML parser delimiter while preserving CSS meaning. */
export function cssForStyle(css) { return css.replaceAll('<', '\\3c '); }
export function themeCss(theme) {
  const [primary,container,onContainer,dark,darkContainer,onDark] = palettes[theme.palette] ?? palettes.violet;
  const [lightNeutral,darkNeutral] = neutrals[theme.palette] ?? neutrals.violet;
  const width = Math.min(1600, Math.max(640, Number(theme.layout.width) || 1120));
  const columns = Math.min(3, Math.max(1, Number(theme.layout.columns) || 2));
  return cssForStyle(`:root{${declarations(lightNeutral)};--primary:${primary};--primary-container:${container};--on-primary-container:${onContainer};--font:${fonts[theme.font] ?? fonts.sans};--content-width:${width}px;--post-columns:${columns}}\n@media(prefers-color-scheme:dark){:root:not([data-appearance="light"]){${declarations(darkNeutral)};--primary:${dark};--primary-container:${darkContainer};--on-primary:${onDark};--on-primary-container:${container}}}\n:root[data-appearance="dark"]{${declarations(darkNeutral)};--primary:${dark};--primary-container:${darkContainer};--on-primary:${onDark};--on-primary-container:${container}}\n${theme.css}`);
}
export function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
}
/** A sandboxed, script-free sample uses the exact stylesheet and layout tokens. */
export function previewHtml(theme, styles, appearance = 'light', icons = '') {
  const esc = escapeHtml, l = theme.layout, rows = homeRows(theme), hasHero = rows.some(row => row.kind === 'hero');
  const blocks = {
    hero: `<section class="hero"><p>静线数据库</p><h1>${esc(l.hero_title || '文章')}</h1><p>${esc(l.hero_description)}</p></section>`,
    posts: `<section><${hasHero ? 'h2' : 'h1'}>文章</${hasHero ? 'h2' : 'h1'}><ul class="post-list">${['一次安静的阅读','把想法写下来','分享一段旅程'].map(title => `<li><article class="post-card"><h2>${title}</h2><p>这是预览示例。文章、页面正文由服务器渲染，主题可以调整字体、宽度和布局。</p><p class="meta">作者 · 2026/10/09</p></article></li>`).join('')}</ul></section>`,
    pages: '<section><h2>页面</h2><ul class="page-directory"><li><a href="#">关于</a></li><li><a href="#">项目</a></li></ul></section>',
  };
  return `<!doctype html><html lang="zh-CN" data-appearance="${appearance === 'dark' ? 'dark' : 'light'}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src 'none'; form-action 'none'"><title>主题预览</title><style>${cssForStyle(styles)}</style><style>${themeCss({ ...theme, css: '' })}</style><style>${cssForStyle(theme.css ?? '')}</style></head><body data-navigation="side" data-post-style="${l.posts === 'list' ? 'list' : 'cards'}">${icons}<header class="site"><span class="brand"><span class="brand-mark"><svg class="icon" aria-hidden="true"><use href="#mi-article"/></svg></span>静线数据库</span><div class="site-dock" data-ready="true" data-expanded="false"><div class="dock-panel"><nav aria-label="示例导航"><a href="#"><span class="dock-symbol"><svg class="icon" aria-hidden="true"><use href="#mi-search"/></svg></span><span class="dock-label">搜索</span></a><a href="#"><span class="dock-symbol"><svg class="icon" aria-hidden="true"><use href="#mi-article"/></svg></span><span class="dock-label">关于</span></a></nav><div class="dock-tools"><button class="dock-tool text">外观</button><button class="dock-tool text">顶部</button><button class="dock-tool text dock-toggle">展开</button></div></div></div></header><main>${rows.map(row => row.kind === 'widgets' ? `<section class="widget-region"><h2>${esc(row.title || '随手探索')}</h2><div class="widget-grid">${row.widgets.map(widget => `<article class="home-widget"><h3>${esc(widget.title || widgetLabels[widget.kind])}</h3><p>${esc(widget.text || '独立加载的首页小组件')}</p></article>`).join('')}</div></section>` : row.kind === 'hero' ? blocks.hero.replace(/<h1>.*?<\/h1>/, () => `<h1>${esc(row.title || l.hero_title || '文章')}</h1>`) : row.kind === 'posts' ? blocks.posts.replace(/>文章<\//, () => `>${esc(row.title || '文章')}</`) : blocks[row.kind] ?? '').join('')}</main><footer class="site">${esc(l.footer)}</footer></body></html>`;
}

export const widgetLabels = { note: '一则便笺', pages: '站点导览', tags: '循着标签', subscribe: '保持联络' };
export function homeRows(theme) {
  if (theme.layout.home_rows?.length) return theme.layout.home_rows.map(row => ({ ...row, widgets: row.widgets ?? [] }));
  return theme.layout.home_blocks.map(kind => ({ kind, title: '', widgets: kind === 'widgets' ? [
    { kind: 'note', title: '留一刻给想法', text: '不赶时间，慢慢阅读。让文字成为日常的一部分。' },
    { kind: 'tags', title: '', text: '' }, { kind: 'subscribe', title: '', text: '' },
  ] : [] }));
}
