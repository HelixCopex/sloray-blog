import { navigate } from 'astro:transitions/client';
import { clientWrite } from './api';
import type { DockContext } from './dock-context';
import { notify } from './toast';
class ActionDialogs extends HTMLElement {
  private controller?: AbortController;
  connectedCallback() {
    this.controller = new AbortController(); const { signal } = this.controller;
    const search = this.querySelector<HTMLDialogElement>('[data-search-dialog]')!;
    const comment = this.querySelector<HTMLDialogElement>('[data-comment-dialog]')!;
    const form = comment.querySelector<HTMLFormElement>('form')!;
    const text = form.elements.namedItem('body_md') as HTMLTextAreaElement;
    const parent = form.elements.namedItem('parent_id') as HTMLSelectElement;
    const nickname = form.elements.namedItem('guest_name') as HTMLInputElement;
    let baseline = JSON.stringify([...new FormData(form)]), submitting = false;
    const dirty = () => JSON.stringify([...new FormData(form)]) !== baseline;
    const dockContext = (): DockContext => JSON.parse(document.querySelector<HTMLElement>('site-dock')?.dataset.context ?? '{"kind":"other"}');
    const approve = () => !submitting && (!dirty() || confirm('有尚未提交的评论，确定放弃并离开？'));
    document.addEventListener('click', event => {
      const button = (event.target as Element).closest('[data-search-open],[data-comment-open]'); if (!button) return;
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey || event.button !== 0) return;
      event.preventDefault();
      if (button.hasAttribute('data-search-open')) { search.showModal(); const input = search.querySelector<HTMLInputElement>('input')!; if (!input.value) input.value = new URL(location.href).searchParams.get('q') ?? ''; input.focus(); }
      else if (dockContext().kind === 'post') {
        if (parent.options.length === 1) {
          const entries = JSON.parse(document.querySelector('[data-comment-parents]')?.textContent ?? '[]') as { id: string; label: string }[];
          entries.forEach(entry => parent.add(new Option(entry.label, entry.id)));
        }
        comment.showModal(); (nickname.value ? text : nickname).focus();
      }
    }, { capture: true, signal });
    this.addEventListener('click', event => {
      const close = (event.target as Element).closest('[data-close-dialog]');
      if (close) { if (submitting && close.closest('dialog') === comment) return; close.closest<HTMLDialogElement>('dialog')?.close(); }
      if (event.target instanceof HTMLDialogElement && !submitting) {
        const rect = event.target.getBoundingClientRect(); if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) event.target.close();
      }
    }, { signal });
    this.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); if (search.open) search.close(); else if (comment.open && !submitting) comment.close(); } }, { signal });
    comment.addEventListener('cancel', event => { if (submitting) event.preventDefault(); }, { signal });
    // Astro treats preventDefault on preparation as a full navigation, not cancellation.
    // Intercept history before the router, then restore the entry without replacing the page.
    let historyIndex = history.state?.index ?? 0, restoring = false;
    let pagePath = location.pathname + location.search;
    document.addEventListener('astro:page-load', () => { historyIndex = history.state?.index ?? historyIndex; pagePath = location.pathname + location.search; }, { signal });
    document.addEventListener('sloray:before-history', event => {
      const pop = (event as CustomEvent<PopStateEvent>).detail;
      if (restoring) { restoring = false; event.preventDefault(); return; }
      const next = pop.state?.index;
      if (next === undefined || pagePath === location.pathname + location.search) return;
      if (!approve()) {
        event.preventDefault(); restoring = true; history.go(historyIndex - next);
        if (!comment.open) comment.showModal();
      } else { historyIndex = next; baseline = JSON.stringify([...new FormData(form)]); }
    }, { signal });
    document.addEventListener('click', event => {
      const link = (event.target as Element).closest<HTMLAnchorElement>('a[href]');
      if (!link || link.hasAttribute('data-search-open') || link.target === '_blank' || link.hasAttribute('download') || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      if (new URL(link.href).pathname === location.pathname && new URL(link.href).search === location.search) return;
      if (!approve()) { event.preventDefault(); event.stopImmediatePropagation(); if (!comment.open) comment.showModal(); }
      else baseline = JSON.stringify([...new FormData(form)]);
    }, { capture: true, signal });
    window.addEventListener('beforeunload', event => { if (dirty() || submitting) { if (!comment.open) comment.showModal(); event.preventDefault(); } }, { signal });
    search.querySelector('form')!.addEventListener('submit', event => {
      event.preventDefault(); if (!approve()) { search.close(); if (!comment.open) comment.showModal(); return; } baseline = JSON.stringify([...new FormData(form)]); const query = search.querySelector<HTMLInputElement>('input')!.value.trim(); if (!query) return;
      const url = `/search?q=${encodeURIComponent(query)}`;
      if (document.querySelector('#editor')) location.assign(url); else void navigate(url);
    }, { signal });
    form.addEventListener('submit', async event => {
      event.preventDefault(); const context = dockContext(); if (submitting || context.kind !== 'post' || !context.slug) return;
      const submit = form.querySelector<HTMLButtonElement>('button[type=submit]')!;
      const controls = [...form.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>('input,select,textarea')];
      const data = new FormData(form); submitting = true; submit.disabled = true; controls.forEach(control => control.disabled = true);
      try {
        await clientWrite(`/posts/${encodeURIComponent(context.slug)}/comments`, 'POST', { guest_name: data.get('guest_name'), body_md: data.get('body_md'), parent_id: data.get('parent_id') || null, website: data.get('website') });
        form.reset(); controls.forEach(control => control.disabled = false); baseline = JSON.stringify([...new FormData(form)]); notify('评论已提交，审核通过后显示。', 'success');
      } catch (error) { notify(String(error), 'error'); }
      finally { submitting = false; submit.disabled = false; controls.forEach(control => control.disabled = false); }
    }, { signal });
  }
  disconnectedCallback() { this.controller?.abort(); }
}
if (!customElements.get('action-dialogs')) customElements.define('action-dialogs', ActionDialogs);
