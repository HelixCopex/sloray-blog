/** Animate the actual content: deferred islands must never be fitted into an earlier snapshot. */
let active: { element: HTMLElement; animation: Animation; restore: () => void } | undefined;
let incoming: { element: HTMLElement; restore: () => void; direction: string } | undefined;
const reduced = matchMedia('(prefers-reduced-motion: reduce)');

function clear() {
  if (active) { active.animation.cancel(); active.restore(); active = undefined; }
  if (incoming) { incoming.restore(); incoming = undefined; }
}
function conceal(element: HTMLElement) {
  const opacity = element.style.getPropertyValue('opacity');
  const priority = element.style.getPropertyPriority('opacity');
  element.style.setProperty('opacity', '0');
  return () => {
    if (opacity) element.style.setProperty('opacity', opacity, priority); else element.style.removeProperty('opacity');
    delete element.dataset.pageMotion;
  };
}
function animate(element: HTMLElement, direction: string, entering: boolean) {
  const sign = direction === 'back' ? -1 : 1;
  const frames = entering ? [{opacity:0,transform:`translateX(${sign * 48}px)`},{opacity:1,transform:'none'}] : [{opacity:1,transform:'none'},{opacity:0,transform:`translateX(${sign * -48}px)`}];
  const style = getComputedStyle(document.documentElement);
  const easing = style.getPropertyValue(entering ? '--md-easing-emphasized-decelerate' : '--md-easing-emphasized-accelerate').trim();
  const animation = element.animate(frames, {duration:entering ? 350 : 200,easing,fill:'both',id:`page-${sign < 0 ? 'back-' : ''}${entering ? 'arrive' : 'away'}`});
  element.dataset.pageMotion = entering ? 'arriving' : 'departing';
  return animation;
}

document.addEventListener('astro:before-preparation', event => {
  clear();
  const loader = event.loader;
  event.loader = async () => {
    await loader();
    if (event.defaultPrevented || event.signal.aborted || reduced.matches) return;
    const element = document.getElementById('content');
    if (!element) return;
    const restore = conceal(element);
    const animation = animate(element, event.direction, false);
    const state = active = {element,animation,restore};
    const cancel = () => { if (active === state) clear(); };
    event.signal.addEventListener('abort', cancel, {once:true});
    try { await animation.finished; } catch { /* A newer navigation restores this page. */ }
  };
});
document.addEventListener('astro:before-swap', event => {
  // Skipping only the screenshot layer retains Astro's swap, focus and history behavior.
  // skipTransition rejects snapshot readiness; the actual update promise remains observable.
  event.viewTransition.ready.catch(() => {});
  event.viewTransition.skipTransition();
  // A previously scheduled native callback must not cancel a newer navigation's motion.
  if (event.signal.aborted) return;
  clear();
  const element = event.newDocument.getElementById('content');
  if (element && !reduced.matches) incoming = {element,restore:conceal(element),direction:event.direction};
});
document.addEventListener('astro:after-swap', () => {
  const state = incoming; incoming = undefined;
  if (!state) return;
  const {element,restore,direction} = state;
  if (!element.isConnected || reduced.matches) { restore(); return; }
  const animation = animate(element, direction, true);
  const current = active = {element,animation,restore};
  animation.finished.then(() => { if (active === current) clear(); }, () => { if (active === current) clear(); });
});
reduced.addEventListener('change', () => { if (reduced.matches) clear(); });
window.addEventListener('pagehide', clear);
