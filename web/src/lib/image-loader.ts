/** A frame has its final geometry before the image loads; decoding only changes its paint. */
class FocusImage extends HTMLElement {
  private controller?: AbortController;
  private observer?: MutationObserver;
  private generation = 0;
  private frame = 0;
  private source = '';
  private identity(img: HTMLImageElement) { return `${img.src}|${img.srcset}|${img.sizes}`; }
  connectedMoveCallback() {}
  connectedCallback() {
    this.controller?.abort(); this.observer?.disconnect();
    this.controller = new AbortController();
    const img = this.querySelector('img'); if (!img) return;
    const { signal } = this.controller;
    img.addEventListener('load', () => void this.reveal(img, this.generation), { signal });
    img.addEventListener('error', () => { this.generation++; cancelAnimationFrame(this.frame); this.dataset.state = 'error'; }, { signal });
    this.observer = new MutationObserver(() => this.reset(img));
    this.observer.observe(img, { attributes: true, attributeFilter: ['src', 'srcset', 'sizes', 'width', 'height'] });
    if (this.dataset.state !== 'ready' || this.source !== this.identity(img) || !img.complete || !img.naturalWidth) this.reset(img);
  }
  private reset(img: HTMLImageElement) {
    const generation = ++this.generation;
    this.source = this.identity(img);
    cancelAnimationFrame(this.frame); this.dataset.state = 'loading';
    if (img.width > 0 && img.height > 0 && img.hasAttribute('width') && img.hasAttribute('height')) {
      this.style.setProperty('--image-aspect', `${img.getAttribute('width')}/${img.getAttribute('height')}`);
      this.style.setProperty('--image-width', `${img.getAttribute('width')}px`);
    }
    if (img.complete && img.naturalWidth > 0) void this.reveal(img, generation);
    else if (img.complete && img.getAttribute('src')) this.dataset.state = 'error';
  }
  private async reveal(img: HTMLImageElement, generation: number) {
    try { await img.decode(); } catch { if (generation === this.generation && this.isConnected) this.dataset.state = 'error'; return; }
    if (generation !== this.generation || !this.isConnected || !img.naturalWidth) return;
    // Give even cached images an initial painted frame before starting the focus transition.
    cancelAnimationFrame(this.frame);
    this.frame = requestAnimationFrame(() => {
      this.frame = requestAnimationFrame(() => { if (generation === this.generation && this.isConnected) this.dataset.state = 'ready'; });
    });
  }
  disconnectedCallback() { this.generation++; cancelAnimationFrame(this.frame); this.controller?.abort(); this.observer?.disconnect(); }
}
if (!customElements.get('focus-image')) customElements.define('focus-image', FocusImage);
