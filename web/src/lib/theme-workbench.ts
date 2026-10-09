import { clientWrite } from './api';
import type { components } from './schema';
import { defaultTheme, defaultBackground, previewHtml } from './theme.mjs';
import material from '../styles/material.css?raw';
import global from '../styles/global.css?raw';
import dock from '../styles/dock.css?raw';
import home from '../styles/home.css?raw';
import icons from '../assets/material-icons.svg?raw';
import { homeComposer } from './home-composer';
import { notify as toast, type ToastKind } from './toast';
type Theme = components['schemas']['Theme'];
type Package = components['schemas']['ThemePackage'];
type State = components['schemas']['ThemeState'];

class ThemeWorkbench extends HTMLElement {
  connectedCallback() {
    if (this.dataset.bound) return;
    this.dataset.bound = 'true';
    let themes = JSON.parse(this.dataset.themes!) as Theme[];
    let state = JSON.parse(this.dataset.state!) as State;
    let selected: Theme | undefined;
    let dirty = false;
    let busy = false;
    const form = this.querySelector<HTMLFormElement>('[data-theme-form]')!;
    const preview = this.querySelector<HTMLIFrameElement>('[data-theme-preview]')!;
    const loadComposer = homeComposer(form);
    const field = (name: string) => form.elements.namedItem(name) as HTMLInputElement;
    const notify = (message: string, kind: ToastKind = 'success') => { if (message) toast(message, kind); };
    const read = (): Package => ({ schema_version: 1, name: field('name').value, description: field('description').value,
      palette: field('palette').value, font: field('font').value, css: field('css').value,
      background: { image_url: field('background_image_url').value.trim(), blur: Number(field('background_blur').value), overlay: Number(field('background_overlay').value), panel_blur: Number(field('background_panel_blur').value) },
      layout: { width: Number(field('width').value), columns: Number(field('columns').value),
        navigation: field('navigation').value, posts: field('posts').value,
        home_blocks: field('home_blocks').value.split(',').map(v => v.trim()).filter(Boolean),
        hero_title: field('hero_title').value, hero_description: field('hero_description').value, footer: field('footer').value,
        ...(JSON.parse(field('home_rows').value || '[]').length ? { home_rows: JSON.parse(field('home_rows').value) } : {}) },
    });
    const updatePreview = () => {
      preview.srcdoc = previewHtml(read(), `@layer material { ${material} }` + global.replace("@import './material.css' layer(material);", '') + dock + home, this.querySelector<HTMLInputElement>('[data-preview-dark]')!.checked ? 'dark' : 'light', icons, location.origin);
    };
    const library = () => {
      this.querySelector('[data-active]')!.textContent = `已启用：${state.package.name}。修改主题库后需要重新启用。`;
      const container = this.querySelector('[data-library]')!; container.replaceChildren();
      themes.forEach(theme => {
        const button = document.createElement('button'); button.type = 'button'; button.textContent = theme.package.name;
        button.disabled = busy;
        button.setAttribute('aria-pressed', String(selected?.id === theme.id));
        button.addEventListener('click', () => { if (!dirty || confirm('放弃尚未保存的主题修改？')) load(theme.package, theme); });
        container.append(button);
      });
      this.querySelector<HTMLButtonElement>('[data-delete]')!.disabled = busy || !selected || selected.id === state.active_theme_id;
      this.querySelector<HTMLButtonElement>('[data-enable]')!.disabled = busy || dirty || !selected;
    };
    const load = (p: Package, theme?: Theme) => {
      selected = theme; dirty = false;
      for (const name of ['name','description','palette','font','css'] as const) field(name).value = p[name] ?? '';
      for (const [name,value] of Object.entries({ ...defaultBackground, ...p.background })) field(`background_${name}`).value = String(value);
      for (const [name,value] of Object.entries(p.layout)) { if (name !== 'home_rows') field(name).value = Array.isArray(value) ? value.join(', ') : String(value); }
      field('home_rows').value = JSON.stringify(p.layout.home_rows ?? []);
      loadComposer(); library(); updatePreview(); notify('');
    };
    const lock = (value: boolean) => {
      busy = value; this.querySelectorAll<HTMLInputElement | HTMLButtonElement>('input,textarea,select,button').forEach(control => control.disabled = value); library();
    };
    const run = async (action: () => Promise<void>) => {
      if (busy) return; lock(true);
      try { await action(); } catch (error) { notify(`操作失败：${String(error)}。如提示版本冲突，请重新打开主题页。`, 'error'); }
      finally { lock(false); }
    };
    let timer: ReturnType<typeof setTimeout>;
    form.addEventListener('input', () => { dirty = true; library(); clearTimeout(timer); timer = setTimeout(updatePreview, 180); });
    this.querySelector('[data-preview-dark]')!.addEventListener('change', updatePreview);
    form.addEventListener('submit', event => {
      event.preventDefault(); if (!form.reportValidity()) return;
      void run(async () => {
        const saved = await clientWrite<Theme>(`/admin/themes${selected ? `/${selected.id}` : ''}`, selected ? 'PUT' : 'POST', { package: read(), version: selected?.version ?? null });
        themes = [saved, ...themes.filter(t => t.id !== saved.id)]; load(saved.package, saved); notify('主题已保存。点击启用可发布到全站。');
      });
    });
    this.querySelector('[data-enable]')!.addEventListener('click', () => { if (selected && !dirty) void run(async () => { state = await clientWrite<State>('/admin/themes/active', 'PUT', { theme_id: selected!.id, theme_version: selected!.version, version: state.version }); library(); notify('主题已启用。公开页面刷新后生效。'); }); });
    this.querySelector('[data-reset]')!.addEventListener('click', () => void run(async () => { state = await clientWrite<State>('/admin/themes/active', 'PUT', { theme_id: null, version: state.version }); library(); notify('已恢复默认主题，主题库仍保留。'); }));
    this.querySelector('[data-new]')!.addEventListener('click', () => { if (!dirty || confirm('放弃尚未保存的主题修改？')) load(structuredClone(defaultTheme)); });
    this.querySelector('[data-delete]')!.addEventListener('click', () => { if (selected && confirm(`删除主题“${selected.package.name}”？`)) void run(async () => { await clientWrite(`/admin/themes/${selected!.id}`, 'DELETE', { version: selected!.version }); themes = themes.filter(t => t.id !== selected!.id); load(structuredClone(defaultTheme)); notify('主题已删除。'); }); });
    this.querySelector('[data-export]')!.addEventListener('click', () => {
      const p = read(), url = URL.createObjectURL(new Blob([JSON.stringify(p, null, 2) + '\n'], { type: 'application/json' }));
      const link = document.createElement('a'); link.href = url; link.download = `${p.name.replace(/[^\p{L}\p{N}_-]/gu, '_') || 'theme'}.json`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
    const input = this.querySelector<HTMLInputElement>('[data-import-file]')!;
    this.querySelector('[data-import]')!.addEventListener('click', () => input.click());
    input.addEventListener('change', () => void run(async () => {
      const file = input.files?.[0]; if (!file) return;
      if (file.size > 100_000) throw new Error('主题包不能超过 100 KB');
      if (dirty && !confirm('放弃尚未保存的主题修改并导入？')) return;
      const p = JSON.parse(await file.text()) as Package;
      // 由服务端统一校验版本、字段和边界；导入只保存，不隐式启用。
      const saved = await clientWrite<Theme>('/admin/themes', 'POST', { package: p, version: null });
      themes = [saved, ...themes]; load(saved.package, saved); notify('主题包已导入，预览满意后点击启用。'); input.value = '';
    }));
    const unload = (event: BeforeUnloadEvent) => { if (dirty) event.preventDefault(); };
    window.addEventListener('beforeunload', unload);
    const leave = (event: MouseEvent) => {
      if (!dirty || event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey || event.button !== 0) return;
      const anchor = (event.target as Element).closest<HTMLAnchorElement>('a[href]');
      if (!anchor || anchor.target || anchor.hasAttribute('download') || anchor.getAttribute('href')?.startsWith('#')) return;
      if (!confirm('放弃尚未保存的主题修改？')) { event.preventDefault(); event.stopImmediatePropagation(); }
      else { dirty = false; anchor.dataset.astroReload = ''; }
    };
    document.addEventListener('click', leave, true);
    this.cleanup = () => { clearTimeout(timer); window.removeEventListener('beforeunload', unload); document.removeEventListener('click', leave, true); };
    load(structuredClone(defaultTheme));
    // 已启用的包是快照；显示库中当前版本，以便编辑后再显式发布。
    const current = themes.find(theme => theme.id === state.active_theme_id); if (current) load(current.package, current);

  }
  private cleanup?: () => void;
  disconnectedCallback() { this.cleanup?.(); }
}
if (!customElements.get('theme-workbench')) customElements.define('theme-workbench', ThemeWorkbench);
