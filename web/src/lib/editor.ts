import { clientWrite, ApiRequestError, type Post, type Page, type WritePost, type WritePage } from './api';

export function bindEditor() {
  const form = document.querySelector<HTMLFormElement>('#editor');
  if (!form || form.dataset.bound) return;
  form.dataset.bound = 'true';
  const notice = document.querySelector<HTMLElement>('#editor-notice')!;
  const kind = form.dataset.kind === 'pages' ? 'pages' : 'posts';
  const label = kind === 'pages' ? '页面' : '文章';
  const status = form.querySelector<HTMLSelectElement>('#status')!;
  const published = form.querySelector<HTMLInputElement>('#published');
  const save = form.querySelector<HTMLButtonElement>('button[type=submit]')!;
  const value = (data: FormData, name: string) => String(data.get(name) ?? '').trim();
  let baseline = JSON.stringify([...new FormData(form)]);
  const dirty = () => JSON.stringify([...new FormData(form)]) !== baseline;
  const syncDock = () => document.dispatchEvent(new Event('editor:sync'));
  function updateStatus() {
    if (published) {
      published.required = status.value === 'scheduled';
      published.closest<HTMLElement>('[data-publish-time]')!.hidden = status.value === 'draft';
      if (status.value === 'published' && published.value && new Date(`${published.value}:00+08:00`).getTime() > Date.now()) published.value = '';
    }
    save.textContent = status.value === 'draft' ? '保存草稿' : status.value === 'scheduled' ? '保存定时发布' : '保存修改';
    syncDock();
  }
  updateStatus(); status.addEventListener('change', updateStatus);
  const beforeUnload = (event: BeforeUnloadEvent) => { if (dirty()) event.preventDefault(); };
  window.addEventListener('beforeunload', beforeUnload);
  const beforeNavigate = (event: MouseEvent) => {
    const link = (event.target as HTMLElement).closest<HTMLAnchorElement>('a[href]');
    if (!link || link.hasAttribute('data-search-open') || link.target === '_blank' || link.hasAttribute('download') || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const target = new URL(link.href);
    if (target.pathname === location.pathname && target.search === location.search) return;
    // 编辑器使用完整导航边界，浏览器前进/后退也会执行原生未保存检查。
    link.dataset.astroReload = 'true';
    if (!dirty()) return;
    if (!confirm('有尚未保存的内容，确定离开编辑器？')) { event.preventDefault(); event.stopImmediatePropagation(); }
    else { window.removeEventListener('beforeunload', beforeUnload); }
  };
  document.addEventListener('click', beforeNavigate, true);
  const keyboard = (event: KeyboardEvent) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); if (!save.disabled) form.requestSubmit(); }
  };
  document.addEventListener('keydown', keyboard);
  document.addEventListener('astro:before-swap', () => {
    window.removeEventListener('beforeunload', beforeUnload);
    document.removeEventListener('click', beforeNavigate, true);
    document.removeEventListener('keydown', keyboard);
  }, { once: true });
  form.addEventListener('submit', async event => {
    event.preventDefault();
    if (save.disabled) return;
    const data = new FormData(form);
    const id = form.dataset.id;
    const common = { slug: value(data, 'slug'), title: value(data, 'title'), summary: value(data, 'summary'), body_md: String(data.get('body_md') ?? ''), status: value(data, 'status'), version: form.dataset.version ? Number(form.dataset.version) : null };
    let body: WritePost | WritePage;
    try {
      const date = value(data, 'published_at');
      body = kind === 'pages' ? { ...common, show_in_nav: data.get('show_in_nav') === 'on', nav_order: Number(value(data, 'nav_order')), dock_icon: value(data, 'dock_icon') || 'article' } : { ...common, published_at: date ? new Date(`${date}:00+08:00`).toISOString() : null, cover_media_id: value(data, 'cover_media_id') || null, lang: 'zh', tags: value(data, 'tags').split(/[,，]/).map(s => s.trim()).filter(Boolean) };
    } catch { notice.textContent = '请输入有效的发布时间。'; return; }
    const controls = [...form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLTextAreaElement | HTMLSelectElement>('input,button,textarea,select')].filter(control => !control.disabled);
    controls.forEach(control => { control.disabled = true; });
    form.dataset.busy = 'true'; syncDock();
    notice.textContent = '正在保存…';
    try {
      const result = await clientWrite<Post | Page>(id === 'new' ? `/admin/${kind}` : `/admin/${kind}/${id}`, id === 'new' ? 'POST' : 'PUT', body);
      form.dataset.id = result.id; form.dataset.version = String(result.version);
      if (published && 'published_at' in result) published.value = result.published_at ? new Date(new Date(result.published_at).getTime() + 8 * 3600 * 1000).toISOString().slice(0, 16) : '';
      controls.forEach(control => { control.disabled = false; });
      baseline = JSON.stringify([...new FormData(form)]);
      notice.textContent = `已保存（版本 ${result.version}）。`;
      const link = document.querySelector<HTMLAnchorElement>('[data-public-link]');
      if (link) { link.href = `${kind === 'pages' ? '/' : '/posts/'}${encodeURIComponent(result.slug)}`; link.hidden = result.status !== 'published'; }
      syncDock();
      if (id === 'new') window.location.assign(`/admin/${kind}/${result.id}`);
    } catch (error) {
      notice.textContent = error instanceof ApiRequestError && error.status === 409 ? `${label}版本已变化或地址已被占用。当前内容已保留，请更换地址或复制内容后重新加载。` : error instanceof Error ? error.message : '保存失败，请重试。';
    } finally { controls.forEach(control => { control.disabled = false; }); delete form.dataset.busy; syncDock(); }
  });
  document.querySelector<HTMLButtonElement>('[data-delete-content]')?.addEventListener('click', async event => {
    if (!confirm(`删除这篇${label}？`)) return;
    const button = event.currentTarget as HTMLButtonElement;
    button.disabled = true; form.dataset.busy = 'true'; syncDock();
    try {
      await clientWrite<void>(`/admin/${kind}/${form.dataset.id}`, 'DELETE', { version: Number(form.dataset.version) });
      baseline = JSON.stringify([...new FormData(form)]);
      window.location.assign(`/admin/${kind}`);
    } catch (error) { notice.textContent = error instanceof Error ? error.message : '删除失败'; button.disabled = false; delete form.dataset.busy; syncDock(); }
  });
}
