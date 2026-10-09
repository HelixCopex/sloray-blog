class SiteDock extends HTMLElement {
  private controller?: AbortController;
  connectedCallback() {
    this.controller?.abort();
    this.controller = new AbortController();
    const { signal } = this.controller;
    const toggle = this.querySelector<HTMLButtonElement>('[data-dock-toggle]')!;
    const links = [...this.querySelectorAll<HTMLAnchorElement>('nav a')];
    const mobile = matchMedia('(max-width: 999px)');
    const expanded = () => this.dataset.expanded === 'true';
    const sync = () => {
      const open = expanded();
      toggle.setAttribute('aria-expanded', String(open));
      toggle.querySelector('span')!.textContent = open ? '收起' : '展开';
      const active = links.findIndex(link => link.getAttribute('aria-current') === 'page');
      links.forEach((link, i) => { link.hidden = mobile.matches && !open && i !== 0 && i !== (active > 1 ? active : 1); });
    };
    const collapse = () => { this.dataset.expanded = 'false'; sync(); };
    this.dataset.ready = 'true'; sync();
    toggle.addEventListener('click', () => { this.dataset.expanded = String(!expanded()); sync(); }, { signal });
    mobile.addEventListener('change', collapse, { signal });
    document.addEventListener('keydown', event => {
      if (event.key === 'Escape' && expanded()) { collapse(); toggle.focus(); }
    }, { signal });
    document.addEventListener('pointerdown', event => {
      if (expanded() && !this.contains(event.target as Node)) collapse();
    }, { signal });
    this.querySelector('[data-back-top]')?.addEventListener('click', () => {
      window.scrollTo({ top: 0, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth' });
      document.querySelector<HTMLElement>('#content')?.focus({ preventScroll: true });
    }, { signal });
  }
  disconnectedCallback() { this.controller?.abort(); }
}
if (!customElements.get('site-dock')) customElements.define('site-dock', SiteDock);
