// One delegated listener survives ClientRouter navigation; ripples never capture input.
document.addEventListener('pointerdown', event => {
  if (event.button !== 0 || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const target = (event.target as Element).closest<HTMLElement>('button:not(:disabled), .button-link, .site-dock nav a');
  if (!target) return;
  target.querySelector('.touch-ripple')?.remove();
  const rect = target.getBoundingClientRect();
  const ripple = document.createElement('span'); ripple.className = 'touch-ripple';
  const size = Math.max(rect.width, rect.height) * 2;
  ripple.style.width = ripple.style.height = `${size}px`;
  ripple.style.left = `${event.clientX - rect.left - size / 2}px`;
  ripple.style.top = `${event.clientY - rect.top - size / 2}px`;
  target.append(ripple); ripple.addEventListener('animationend', () => ripple.remove(), { once: true });
});
