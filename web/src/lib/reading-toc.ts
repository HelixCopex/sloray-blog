/** 增强 SSR 目录；滚动、字号/图片变化、群岛重载和局部导航均重新同步。 */
class ReadingToc extends HTMLElement {
  private cleanup?: () => void;
  connectedCallback() {
    this.cleanup?.();
    const prose = this.closest('article')?.querySelector<HTMLElement>('.prose');
    if (!prose) return;
    const controller = new AbortController(), { signal } = controller;
    const details = this.querySelector<HTMLDetailsElement>('details')!;
    const list = this.querySelector<HTMLOListElement>('[data-toc-list]')!;
    const current = this.querySelector<HTMLElement>('[data-toc-current]')!;
    const mobile = matchMedia('(max-width: 999px)');
    let headings: HTMLElement[] = [], links: HTMLAnchorElement[] = [], frame = 0, active = '';
    const syncMenu = () => { details.open = !mobile.matches; };
    const update = () => {
      if (!headings.length) return;
      // 锚点布局可能落在小数像素；容差防止跳转后仍高亮上一章。
      const threshold = Math.max(mobile.matches ? 112 : 64, parseFloat(getComputedStyle(headings[0]).scrollMarginTop) || 0) + 1;
      let index = 0;
      headings.forEach((heading, i) => { if (heading.getBoundingClientRect().top <= threshold) index = i; });
      const id = headings[index].id;
      if (active === id) return;
      active = id;
      links.forEach((link, i) => {
        if (i === index) link.setAttribute('aria-current', 'location');
        else link.removeAttribute('aria-current');
      });
      current.textContent = headings[index].textContent;
      // 仅滚动目录自身，避免 scrollIntoView 意外拖动正文。
      const link = links[index], nav = list.parentElement!;
      if (!mobile.matches || details.open) {
        const top = link.offsetTop - nav.offsetTop;
        if (top < nav.scrollTop || top + link.offsetHeight > nav.scrollTop + nav.clientHeight)
          nav.scrollTop = Math.max(0, top - nav.clientHeight / 2);
      }
    };
    const queue = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; update(); }); };
    const rebuild = () => {
      headings = [...prose.querySelectorAll<HTMLElement>('h2,h3,h4,h5,h6')];
      const used = new Set([...document.querySelectorAll<HTMLElement>('[id]')].map(el => el.id));
      const base = Math.min(...headings.map(heading => Number(heading.tagName.slice(1))));
      links = headings.map((heading, i) => {
        if (!heading.id) {
          let id = `reading-heading-${i}`;
          while (used.has(id)) id += '-';
          heading.id = id; used.add(id);
        }
        const li = document.createElement('li'), link = document.createElement('a');
        li.style.setProperty('--toc-depth', String(Number(heading.tagName.slice(1)) - base));
        link.href = `#${encodeURIComponent(heading.id)}`; link.dataset.headingId = heading.id;
        link.textContent = heading.textContent; li.append(link); return link;
      });
      list.replaceChildren(...links.map(link => link.parentElement!));
      this.hidden = headings.length === 0; active = ''; update();
    };
    list.addEventListener('click', event => {
      const link = (event.target as Element).closest<HTMLAnchorElement>('a');
      const heading = link && document.getElementById(link.dataset.headingId!);
      if (!heading) return;
      if (mobile.matches) details.open = false;
      // 原生锚点保留 URL 和历史；标题接收焦点，键盘阅读可直接继续。
      heading.tabIndex = -1; heading.focus({ preventScroll: true });
    }, { signal });
    details.addEventListener('keydown', event => {
      if (mobile.matches && event.key === 'Escape') { details.open = false; details.querySelector('summary')?.focus(); }
    }, { signal });
    details.addEventListener('toggle', queue, { signal });
    mobile.addEventListener('change', syncMenu, { signal });
    window.addEventListener('scroll', queue, { passive: true, signal });
    window.addEventListener('resize', queue, { signal });
    const resize = new ResizeObserver(queue); resize.observe(prose);
    const mutation = new MutationObserver(rebuild);
    mutation.observe(prose, { subtree: true, childList: true, characterData: true });
    syncMenu(); rebuild(); this.dataset.ready = 'true';
    this.cleanup = () => { controller.abort(); resize.disconnect(); mutation.disconnect(); cancelAnimationFrame(frame); };
  }
  disconnectedCallback() { this.cleanup?.(); }
}
if (!customElements.get('reading-toc')) customElements.define('reading-toc', ReadingToc);
