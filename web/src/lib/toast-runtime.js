// Shared by Astro and the self-contained Rust setup/login/settings pages.
(() => {
  if (window.__slorayToasts) return;
  window.__slorayToasts = true;
  const timers = new WeakMap();
  function stack() {
    let host = document.querySelector('toast-stack');
    if (!host) {
      host = document.createElement('toast-stack'); host.className = 'toast-stack';
      host.setAttribute('popover', 'manual'); host.setAttribute('aria-label', '操作通知'); document.body.append(host);
    }
    return host;
  }
  function open(host) {
    const parent = [...document.querySelectorAll('dialog:modal')].at(-1) ?? document.body;
    // Modal descendants remain interactive; a sibling popover is still inert.
    if (host.matches(':popover-open')) host.hidePopover();
    if (host.parentElement !== parent) parent.append(host);
    if (host.children.length) host.showPopover?.();
  }
  function show({ message, kind = 'info', id, persist = false }) {
    if (!message) return;
    if (persist) { try { sessionStorage.setItem('sloray-next-toast', JSON.stringify({ message, kind })); } catch {} }
    const host = stack();
    const previous = [...host.children].find(node => id ? node.dataset.id === id : node.dataset.message === message && node.dataset.kind === kind);
    if (previous) { clearTimeout(timers.get(previous)); previous.remove(); }
    while (host.children.length >= 4) { const first = host.firstElementChild; clearTimeout(timers.get(first)); first.remove(); }
    const card = document.createElement('div'); card.className = 'toast';
    card.dataset.kind = ['info','success','warning','error'].includes(kind) ? kind : 'error';
    card.dataset.message = message; if (id) card.dataset.id = id;
    card.setAttribute('role', kind === 'error' ? 'alert' : 'status'); card.setAttribute('aria-atomic', 'true');
    const text = document.createElement('p'); text.className = 'toast-message'; text.textContent = message;
    const close = document.createElement('button'); close.type = 'button'; close.className = 'toast-close'; close.setAttribute('aria-label', '关闭通知'); close.textContent = '×';
    const previousFocus = document.activeElement;
    function remove() {
      clearTimeout(timers.get(card)); const focused = card.contains(document.activeElement); card.remove();
      if (!host.children.length && host.matches(':popover-open')) host.hidePopover();
      if (focused && previousFocus?.isConnected) previousFocus.focus({ preventScroll: true });
    }
    const schedule = () => { clearTimeout(timers.get(card)); timers.set(card, setTimeout(remove, kind === 'error' || kind === 'warning' ? 12000 : 6000)); };
    close.addEventListener('click', remove);
    card.addEventListener('pointerenter', () => clearTimeout(timers.get(card)));
    card.addEventListener('focusin', () => clearTimeout(timers.get(card)));
    card.addEventListener('pointerleave', () => { if (!card.contains(document.activeElement)) schedule(); });
    card.addEventListener('focusout', schedule);
    card.append(text, close); host.append(card); open(host); schedule();
  }
  const sourceSelector = '.notice, p[role=alert], [data-toast-source]';
  function consume(root = document) {
    const sources = [...root.querySelectorAll(sourceSelector)];
    if (root.matches?.(sourceSelector)) sources.unshift(root);
    sources.forEach(node => {
      if (node.dataset.toastConsumed || !node.textContent.trim()) return;
      node.dataset.toastConsumed = 'true'; node.hidden = true;
      const kind = node.dataset.toastKind ?? (node.classList.contains('ok') ? 'success' : node.classList.contains('warn') ? 'warning' : 'error');
      show({ message: node.textContent.trim(), kind });
    });
  }
  window.addEventListener('sloray:toast', event => show(event.detail));
  function init() {
    consume();
    try { const next = sessionStorage.getItem('sloray-next-toast'); sessionStorage.removeItem('sloray-next-toast'); if (next) show(JSON.parse(next)); } catch {}
    const observer = new MutationObserver(records => records.forEach(record => record.addedNodes.forEach(node => { if (node.nodeType === 1) consume(node); })));
    observer.observe(document.body, {childList:true,subtree:true});
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init, {once:true}); else init();
  document.addEventListener('astro:after-swap', () => { consume(); open(stack()); });
  document.addEventListener('close', event => { if (event.target.matches?.('dialog')) open(stack()); }, true);
  document.addEventListener('toggle', event => { if (event.target.matches?.('dialog')) open(stack()); }, true);
})();
