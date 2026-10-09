import { clientGet, clientWrite, ApiRequestError } from './api';
import type { components } from './schema';
import type { DockContext } from './dock-context';

// Reconcile by action identity: hovering buttons and their ripples retain the same DOM nodes.
function reconcile(current: Element, incoming: Element) {
  const retained = new Map([...current.children].map(node => [node.getAttribute('data-dock-key'), node]));
  const wanted: Element[] = [];
  for (const next of incoming.children) {
    const previous = retained.get(next.getAttribute('data-dock-key'));
    if (!previous || previous.tagName !== next.tagName) { wanted.push(next.cloneNode(true) as Element); continue; }
    for (const attr of [...previous.attributes]) if (!next.hasAttribute(attr.name) && attr.name !== 'hidden') previous.removeAttribute(attr.name);
    for (const attr of [...next.attributes]) previous.setAttribute(attr.name, attr.value);
    const oldIcon = previous.querySelector('use'), newIcon = next.querySelector('use');
    if (oldIcon && newIcon) { oldIcon.setAttribute('href', newIcon.getAttribute('href')!); oldIcon.closest('svg')?.setAttribute('data-icon', newIcon.closest('svg')?.getAttribute('data-icon') ?? ''); }
    const oldImage = previous.querySelector('img'), newImage = next.querySelector('img');
    if (oldImage && newImage && oldImage.src !== newImage.src) { oldImage.src = newImage.src; oldImage.hidden = false; }
    wanted.push(previous);
  }
  for (const child of [...current.children]) if (!wanted.includes(child)) child.remove();
  wanted.forEach((node, index) => { if (current.children[index] !== node) current.insertBefore(node, current.children[index] ?? null); });
}

class SiteDock extends HTMLElement {
  private controller?: AbortController;
  private epoch = 0;
  private liked = false;
  private context(): DockContext { return JSON.parse(this.dataset.context ?? '{"kind":"other"}'); }
  connectedMoveCallback() {} // Native moveBefore preserves focus, custom-element state and animations.
  connectedCallback() {
    this.controller?.abort(); this.controller = new AbortController();
    const { signal } = this.controller;
    const mobile = matchMedia('(max-width: 999px)');
    this.dataset.ready = 'true'; this.sync();
    this.addEventListener('error', event => { if ((event.target as HTMLElement)?.matches('.dock-favicon')) (event.target as HTMLElement).hidden = true; }, { capture: true, signal });
    this.addEventListener('click', event => {
      const button = (event.target as Element).closest<HTMLElement>('[data-dock-toggle],[data-back-top],[data-like],[data-editor-action]');
      if (button?.hasAttribute('data-dock-toggle')) { this.dataset.expanded = String(this.dataset.expanded !== 'true'); this.sync(); this.syncEditor(); }
      if (button?.hasAttribute('data-back-top')) { window.scrollTo({ top: 0, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' }); document.querySelector<HTMLElement>('#content')?.focus({ preventScroll: true }); }
      if (button?.hasAttribute('data-like')) void this.like(button as HTMLButtonElement);
      if (button?.dataset.editorAction) this.editorAction(button.dataset.editorAction);
    }, { signal });
    mobile.addEventListener('change', () => { this.dataset.expanded = 'false'; this.sync(); }, { signal });
    document.addEventListener('keydown', event => { if (event.key === 'Escape' && this.dataset.expanded === 'true') { this.dataset.expanded = 'false'; this.sync(); this.querySelector<HTMLElement>('[data-dock-toggle]')?.focus(); } }, { signal });
    document.addEventListener('pointerdown', event => { if (!this.contains(event.target as Node)) { this.dataset.expanded = 'false'; this.sync(); } }, { signal });
    document.addEventListener('editor:sync', () => this.syncEditor(), { signal });
    document.addEventListener('astro:before-swap', event => {
      const incoming = event.newDocument.querySelector('site-dock'); if (!incoming) return;
      reconcile(this.querySelector('nav')!, incoming.querySelector('nav')!);
      reconcile(this.querySelector('.dock-context')!, incoming.querySelector('.dock-context')!);
      this.dataset.context = (incoming as HTMLElement).dataset.context;
      this.epoch++; this.liked = false; this.dataset.expanded = 'false'; this.sync();
      this.querySelector('[data-dock-notice]')!.textContent = '';
      // Fallback browsers relocate persisted elements using remove/append. Restore timelines after the swap.
      const timelines = this.getAnimations({ subtree: true }).map(animation => ({ animation, time: animation.currentTime, state: animation.playState, target: (animation.effect as KeyframeEffect | null)?.target, name: animation instanceof CSSAnimation ? animation.animationName : animation instanceof CSSTransition ? animation.transitionProperty : undefined }));
      const swap = event.swap;
      event.swap = () => {
        swap(); const current = this.getAnimations({ subtree: true });
        for (const snapshot of timelines) {
          if (snapshot.state === 'finished' || snapshot.time === null) continue;
          const animation = snapshot.name ? current.find(candidate => (candidate.effect as KeyframeEffect | null)?.target === snapshot.target && (candidate instanceof CSSAnimation ? candidate.animationName : candidate instanceof CSSTransition ? candidate.transitionProperty : undefined) === snapshot.name) ?? snapshot.animation : snapshot.animation;
          animation.currentTime = snapshot.time;
          if (snapshot.state === 'running') animation.play(); else if (snapshot.state === 'paused') animation.pause();
        }
      };
    }, { signal });
    document.addEventListener('astro:page-load', () => { this.syncEditor(); void this.stats(true); }, { signal });
    this.syncEditor();
  }
  private sync() {
    const open = this.dataset.expanded === 'true', mobile = matchMedia('(max-width: 999px)').matches;
    const toggle = this.querySelector<HTMLElement>('[data-dock-toggle]')!;
    toggle.setAttribute('aria-expanded', String(open)); toggle.setAttribute('aria-label', open ? '收起导航' : '展开导航'); toggle.title = open ? '收起导航' : '展开导航';
    this.querySelectorAll<HTMLElement>('nav a').forEach(link => { link.hidden = mobile && !open && !['home','search'].includes(link.dataset.dockKey ?? ''); });
    this.querySelectorAll<HTMLElement>('.dock-context > *').forEach(button => { if (!button.hasAttribute('data-editor-preview')) button.hidden = mobile && !open && !button.matches('[data-like],[data-comment-open]'); });
  }
  private async stats(view = false) {
    const context = this.context(), epoch = this.epoch;
    if (context.kind !== 'post' || !context.id) return;
    const section = document.querySelector<HTMLElement>('.discussion'); if (section) section.dataset.bound = 'true';
    try {
      if (view) await clientWrite(`/posts/${context.id}/view`, 'POST', {});
      const stats = await clientGet<components['schemas']['Stats']>(`/posts/${context.id}/stats`);
      if (epoch !== this.epoch) return;
      this.liked = stats.liked;
      const button = this.querySelector<HTMLButtonElement>('[data-like]'); if (button) button.disabled = false;
      button?.setAttribute('aria-pressed', String(stats.liked)); button?.setAttribute('aria-label', stats.liked ? '取消点赞' : '点赞'); if (button) button.title = `${stats.liked ? '取消点赞' : '点赞'} · ${stats.like_count}`;
      const counts = document.querySelector('[data-stats]'); if (counts) counts.textContent = `${stats.view_count} 次浏览 · ${stats.like_count} 人点赞`;
    } catch { if (epoch === this.epoch) { const counts = document.querySelector('[data-stats]'); if (counts) counts.textContent = '统计暂时不可用'; } }
  }
  private async like(button: HTMLButtonElement) {
    const context = this.context(), epoch = this.epoch; if (!context.id || button.disabled) return;
    button.disabled = true;
    try { await clientWrite(`/posts/${context.id}/like`, this.liked ? 'DELETE' : 'PUT', {}); if (epoch === this.epoch) await this.stats(); }
    catch (error) { if (epoch === this.epoch) this.querySelector('[data-dock-notice]')!.textContent = error instanceof ApiRequestError && error.status === 401 ? '请登录后点赞。' : String(error); }
    finally { if (epoch === this.epoch) button.disabled = false; }
  }
  private editorAction(action: string) {
    const form = document.querySelector<HTMLFormElement>('#editor'); if (!form || form.dataset.busy === 'true') return;
    if (action === 'save' || action === 'publish') {
      if (action === 'publish') { const status = form.querySelector<HTMLSelectElement>('#status')!; status.value = 'published'; status.dispatchEvent(new Event('change')); }
      form.requestSubmit(form.querySelector<HTMLButtonElement>('button[type=submit]')!);
    } else document.querySelector<HTMLButtonElement>(action === 'media' ? '[data-open-media]' : '[data-delete-content]')?.click();
  }
  private syncEditor() {
    const form = document.querySelector<HTMLFormElement>('#editor');
    this.querySelectorAll<HTMLButtonElement>('[data-editor-action]').forEach(button => { button.disabled = !form || form.dataset.busy === 'true'; });
    if (!form) return;
    const status = form.querySelector<HTMLSelectElement>('#status')?.value;
    const label = status === 'draft' ? '保存草稿' : status === 'scheduled' ? '保存定时发布' : '保存修改';
    const save = this.querySelector<HTMLElement>('[data-editor-action=save]'); save?.setAttribute('aria-label', label); if (save) save.title = label;
    const original = form.querySelector<HTMLAnchorElement>('[data-public-link]'), preview = this.querySelector<HTMLAnchorElement>('[data-editor-preview]');
    if (preview && original) { preview.href = original.href; preview.hidden = original.hidden || matchMedia('(max-width: 999px)').matches && this.dataset.expanded !== 'true'; }
  }
  disconnectedCallback() { this.controller?.abort(); }
}
if (!customElements.get('site-dock')) customElements.define('site-dock', SiteDock);
