import { clientGet, type Session } from './api';
import { validDimensions, type ContentImage, type ImageDimensions } from './markdown-images';
import type { components } from './schema';

function measureImage(url: string): Promise<ImageDimensions> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    const timeout = setTimeout(fail, 8000);
    function cleanup() { clearTimeout(timeout); image.onload = image.onerror = null; }
    function fail() { cleanup(); image.removeAttribute('src'); reject(new Error('无法读取正文图片的尺寸。请检查图片地址，或在链接后填写 {width=原始宽度 height=原始高度}，再保存。')); }
    image.onload = () => {
      const width = image.naturalWidth, height = image.naturalHeight;
      if (!validDimensions(width, height)) { fail(); return; }
      cleanup(); resolve({ width, height });
    };
    image.onerror = fail;
    image.src = url;
  });
}
/** AST offsets from the shared renderer keep fenced code, escaped images and references intact. */
export async function prepareImageDimensions(markdown: string): Promise<{ markdown: string; unresolved: number }> {
  if (!markdown.includes('!')) return { markdown, unresolved: 0 };
  const session = await clientGet<Session>('/auth/session');
  const response = await fetch('/admin/preview', { method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json', 'x-csrf-token': session.csrf_token }, body: JSON.stringify({ body_md: markdown }) });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error?.message ?? '读取正文图片失败，请重试。');
  const missing = (result.images as ContentImage[]).filter(image => !validDimensions(image.width, image.height));
  const known = new Map<string, ImageDimensions>();
  const absolute = (url: string) => new URL(url, location.href).href;
  if (missing.length) {
    // Metadata remains usable even if storage is temporarily unavailable or blocks a new request.
    try {
      const media = await clientGet<components['schemas']['Media'][]>('/admin/media');
      for (const item of media) if (validDimensions(item.width, item.height)) for (const variant of item.variants) known.set(absolute(variant.url), { width: item.width!, height: item.height! });
    } catch { /* Authors without library access can still measure public images. */ }
    document.querySelectorAll<HTMLImageElement>('[data-markdown-preview] img, img.cover').forEach(image => {
      if (image.complete && validDimensions(image.naturalWidth, image.naturalHeight) && !known.has(absolute(image.src))) known.set(absolute(image.src), {width:image.naturalWidth,height:image.naturalHeight});
    });
  }
  const dimensions = new Map<string, Promise<ImageDimensions>>();
  let cursor = 0, unresolved = 0;
  const replacements: { end: number; dimensions: ImageDimensions }[] = [];
  await Promise.all(Array.from({ length: Math.min(4, missing.length) }, async () => {
    while (cursor < missing.length) {
      const image = missing[cursor++];
      let promise = dimensions.get(image.url);
      if (!promise) { const metadata = known.get(absolute(image.url)); promise = metadata ? Promise.resolve(metadata) : measureImage(image.url); dimensions.set(image.url, promise); }
      try { replacements.push({ end: image.end, dimensions: await promise }); } catch { unresolved++; }
    }
  }));
  for (const { end, dimensions: { width, height } } of replacements.sort((a, b) => b.end - a.end)) markdown = `${markdown.slice(0, end)}{width=${width} height=${height}}${markdown.slice(end)}`;
  return { markdown, unresolved };
}
