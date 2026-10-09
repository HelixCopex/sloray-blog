class ReadingProgress extends HTMLElement {
  private cleanup?: () => void;
  connectedCallback() {
    const prose = this.closest('article')?.querySelector<HTMLElement>('.prose'); if (!prose) return;
    const controller = new AbortController(), { signal } = controller;
    const progress = this.querySelector<HTMLProgressElement>('progress')!;
    let frame = 0;
    const update = () => { progress.value = Math.max(0, Math.min(100, (-prose.getBoundingClientRect().top) / Math.max(1, prose.offsetHeight - innerHeight) * 100)); };
    const queue = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; update(); }); };
    window.addEventListener('scroll', queue, { passive: true, signal }); window.addEventListener('resize', queue, { signal });
    const observer = new ResizeObserver(queue); observer.observe(prose); update();
    this.cleanup = () => { controller.abort(); observer.disconnect(); cancelAnimationFrame(frame); };
  }
  disconnectedCallback() { this.cleanup?.(); }
}
if (!customElements.get('reading-progress')) customElements.define('reading-progress', ReadingProgress);
