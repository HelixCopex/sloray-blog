class WidgetCard extends HTMLElement {
  private controller?: AbortController;
  private observer?: MutationObserver;
  connectedCallback() {
    const button = this.querySelector<HTMLButtonElement>('[data-widget-refresh]')!;
    const content = this.querySelector<HTMLElement>('[data-widget-content]')!;
    const status = this.querySelector<HTMLElement>('[data-widget-status]')!;
    const ready = () => { button.disabled = !content.querySelector('[data-widget-loaded]'); };
    this.observer = new MutationObserver(ready); this.observer.observe(content, { subtree: true, childList: true }); ready();
    this.controller = new AbortController();
    button.addEventListener('click', async () => {
      button.disabled = true; this.observer?.disconnect(); this.setAttribute('aria-busy', 'true'); status.textContent = '正在刷新…';
      try {
        const response = await fetch(this.dataset.source!, { credentials: 'same-origin', signal: this.controller!.signal });
        if (!response.ok) throw new Error('小组件配置可能已改变，请刷新页面后重试。');
        const html = await response.text();
        if (!this.isConnected) return;
        content.innerHTML = html; status.textContent = '已更新';
      } catch (error) {
        if (!this.controller?.signal.aborted) status.textContent = error instanceof Error ? error.message : '刷新失败，点击重试。';
      } finally { if (this.isConnected) { button.disabled = false; this.removeAttribute('aria-busy'); } }
    }, { signal: this.controller.signal });
  }
  disconnectedCallback() { this.controller?.abort(); this.observer?.disconnect(); }
}
if (!customElements.get('widget-card')) customElements.define('widget-card', WidgetCard);
