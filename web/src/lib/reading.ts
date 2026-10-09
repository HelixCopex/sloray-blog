class ReadingTools extends HTMLElement {
  private cleanup?: () => void;
  connectedCallback() {
    const prose = this.closest('article')?.querySelector<HTMLElement>('.prose'); if (!prose) return;
    const controller = new AbortController(), { signal } = controller;
    const select = this.querySelector<HTMLSelectElement>('[data-reading-size]')!;
    const focus = this.querySelector<HTMLButtonElement>('[data-reading-focus]')!;
    const progress = this.querySelector<HTMLProgressElement>('[data-reading-progress]')!;
    const status = this.querySelector<HTMLElement>('[data-reading-status]')!;
    const applySize = () => { prose.style.fontSize = `${Number(select.value) || 1}rem`; };
    try { const stored = localStorage.getItem('sloray-reading-size'); if (['1','1.12','1.25'].includes(stored ?? '')) select.value = stored!; } catch {}
    applySize();
    select.addEventListener('change', () => { applySize(); try { localStorage.setItem('sloray-reading-size', select.value); } catch {} }, { signal });
    focus.addEventListener('click', () => {
      const value = focus.getAttribute('aria-pressed') !== 'true'; focus.setAttribute('aria-pressed', String(value));
      document.body.toggleAttribute('data-reading-focus', value);
    }, { signal });
    const headings = [...prose.querySelectorAll<HTMLElement>('h2,h3,h4,h5,h6')];
    const toc = this.querySelector('[data-reading-toc]')!;
    headings.forEach((heading, i) => {
      if (!heading.id) heading.id = `reading-heading-${i}`;
      const li = document.createElement('li'), a = document.createElement('a');
      a.href = `#${heading.id}`; a.textContent = heading.textContent; li.append(a); toc.append(li);
    });
    this.querySelector<HTMLDetailsElement>('details')!.hidden = headings.length === 0;
    const update = () => {
      const top = prose.getBoundingClientRect().top + scrollY;
      const span = prose.offsetHeight - innerHeight;
      progress.value = Math.max(0, Math.min(100, (scrollY - top) / Math.max(1, span) * 100));
    };
    let frame = 0;
    const queue = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; update(); }); };
    window.addEventListener('scroll', queue, { passive: true, signal });
    window.addEventListener('resize', queue, { signal });
    const observer = new ResizeObserver(queue); observer.observe(prose); update();
    prose.querySelectorAll('pre').forEach(pre => {
      const code = pre.querySelector('code'); if (!code) return;
      const button = document.createElement('button'); button.type = 'button'; button.className = 'copy-code text'; button.textContent = '复制代码'; pre.before(button);
      button.addEventListener('click', async () => {
        try { await navigator.clipboard.writeText(code.textContent ?? ''); status.textContent = '代码已复制'; }
        catch { status.textContent = '浏览器未允许复制，请手动选择代码。'; }
      }, { signal });
    });
    this.cleanup = () => { controller.abort(); observer.disconnect(); cancelAnimationFrame(frame); document.body.removeAttribute('data-reading-focus'); };
  }
  disconnectedCallback() { this.cleanup?.(); }
}
if (!customElements.get('reading-tools')) customElements.define('reading-tools', ReadingTools);
