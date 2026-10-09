import { mediaMarkdown } from './media-markdown.mjs';
import { clientGet, clientWrite } from './api';
import { describeUploadFailure, MediaUploadError, uploadOriginal, type UploadContext } from './media-upload';
import type { components } from './schema';
type Media = components['schemas']['Media'];
type Presigned = components['schemas']['Presigned'];

export function bindMediaPicker() {
  const root = document.querySelector<HTMLElement>('[data-media-picker]');
  if (!root || root.dataset.bound) return;
  root.dataset.bound = 'true';
  const dialog = root.closest<HTMLDialogElement>('dialog');
  const list = root.querySelector<HTMLElement>('[data-media-list]')!;
  const libraryNotice = root.querySelector<HTMLElement>('[data-library-notice]')!;
  const notice = root.querySelector<HTMLElement>('[data-media-notice]')!;
  const input = root.querySelector<HTMLInputElement>('#image-file')!;
  const upload = root.querySelector<HTMLButtonElement>('[data-upload]')!;
  const refresh = root.querySelector<HTMLButtonElement>('[data-refresh-media]')!;
  const progress = root.querySelector<HTMLProgressElement>('progress')!;
  const errorPanel = root.querySelector<HTMLDetailsElement>('[data-media-error]')!;
  const errorDetails = root.querySelector<HTMLElement>('[data-media-error-details]')!;
  const alt = root.querySelector<HTMLInputElement>('#image-alt')!;
  const copyField = root.querySelector<HTMLTextAreaElement>('[data-markdown-copy]')!;
  let media: Media[] = [];
  const button = (label: string, action: string, id: string) => {
    const element = document.createElement('button');
    element.type = 'button'; element.textContent = label; element.dataset.action = action; element.dataset.id = id;
    return element;
  };
  function render() {
    list.replaceChildren();
    for (const item of media) {
      const card = document.createElement('li'); card.dataset.id = item.id;
      const image = item.variants.at(-1);
      if (item.ready && image) {
        const copy = button('复制图片 Markdown', 'copy', item.id); copy.className = 'media-image'; copy.setAttribute('aria-label', '复制图片 Markdown');
        const img = document.createElement('img'); img.src = image.url; img.alt = '点击复制 Markdown 图片链接'; img.loading = 'lazy';
        copy.prepend(img); card.append(copy);
        if (root!.dataset.allowCover === 'true') card.append(button('设为封面', 'cover', item.id));
      }
      const description = document.createElement('p'); description.className = 'hint';
      description.textContent = `${Math.ceil(item.size_bytes / 1024)} KiB · ${item.ready && image ? `${item.width} × ${item.height}` : '原图已上传，等待图片处理完成；稍后刷新媒体。'}`;
      card.append(description);
      if (root!.dataset.manage === 'true') card.append(button('删除图片', 'delete', item.id));
      list.append(card);
    }
    if (!media.length) libraryNotice.textContent = '媒体库还没有图片，请先上传。';
  }
  async function load() {
    refresh.disabled = true;
    libraryNotice.textContent = '正在加载媒体…';
    try {
      media = await clientGet<Media[]>('/admin/media');
      if (!root!.isConnected) return;
      libraryNotice.textContent = ''; render();
    } catch (error) { libraryNotice.textContent = error instanceof Error ? error.message : '加载失败，请刷新重试。'; }
    finally { refresh.disabled = false; }
  }
  document.querySelectorAll<HTMLButtonElement>('[data-open-media]').forEach(open => open.addEventListener('click', () => { dialog?.showModal(); void load(); }));
  dialog?.querySelector<HTMLButtonElement>('[data-close-media]')?.addEventListener('click', () => dialog.close());
  refresh.addEventListener('click', () => void load());
  if (!dialog) void load();
  list.addEventListener('click', async event => {
    const target = (event.target as HTMLElement).closest<HTMLButtonElement>('button[data-action]');
    if (!target) return;
    const item = media.find(item => item.id === target.dataset.id);
    if (!item) return;
    const image = item.variants.at(-1);
    if (target.dataset.action === 'copy' && image) {
      copyField.value = mediaMarkdown(alt.value, image.url);
      root.querySelector<HTMLElement>('[data-copy-result]')!.hidden = false;
      try { await navigator.clipboard.writeText(copyField.value); libraryNotice.textContent = 'Markdown 图片链接已复制，返回正文粘贴即可。'; }
      catch {
        copyField.focus(); copyField.select();
        libraryNotice.textContent = '浏览器未允许自动复制，请复制下方已选中的 Markdown 链接。';
      }
    } else if (target.dataset.action === 'cover' && image) {
      const cover = document.querySelector<HTMLInputElement>('#cover-media');
      if (!cover) return;
      cover.value = item.id; cover.dispatchEvent(new Event('input', { bubbles: true }));
      const preview = document.querySelector<HTMLImageElement>('[data-cover-preview]')!;
      preview.src = image.url; preview.hidden = false;
      document.querySelector<HTMLElement>('[data-cover-status]')!.textContent = '封面已选择，保存文章后生效。';
      document.querySelector<HTMLButtonElement>('[data-clear-cover]')!.hidden = false;
      dialog?.close();
    } else if (target.dataset.action === 'delete') {
      if (!confirm('删除图片并移除使用它的文章封面？正文中的旧链接需要手动移除。')) return;
      target.disabled = true;
      try { await clientWrite(`/admin/media/${item.id}`, 'DELETE', {}); media = media.filter(m => m.id !== item.id); render(); libraryNotice.textContent = '图片已删除。'; }
      catch (error) { libraryNotice.textContent = String(error); target.disabled = false; }
    }
  });
  upload.addEventListener('click', async () => {
    errorPanel.hidden = true; errorPanel.open = false; errorDetails.textContent = '';
    const file = input.files?.[0];
    if (!file) { notice.textContent = '请先选择图片。'; return; }
    if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) { notice.textContent = '请选择 PNG、JPEG 或 WebP 图片。'; return; }
    if (file.size > 10 * 1024 * 1024) { notice.textContent = '图片超过 10 MiB。'; return; }
    upload.disabled = true; notice.textContent = '准备上传图片…'; progress.value = 0;
    const context: UploadContext = { stage: '校验图片', origin: window.location.origin, contentType: file.type, sizeBytes: file.size };
    try {
      const sha256 = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer())), b => b.toString(16).padStart(2, '0')).join('');
      context.stage = '申请上传许可';
      const signed = await clientWrite<Presigned>('/admin/media/presign', 'POST', { content_type: file.type, size_bytes: file.size, sha256 });
      context.uploadId = signed.upload_id ?? undefined; context.uploadUrl = signed.upload_url ?? undefined; context.headers = signed.headers;
      let item = signed.media ?? undefined;
      if (!item) {
        context.stage = '原图上传'; progress.hidden = false;
        await uploadOriginal(signed.upload_url!, file, signed.headers, percent => { progress.value = percent; });
        context.stage = '登记媒体';
        item = await clientWrite<Media>('/admin/media', 'POST', { upload_id: signed.upload_id });
        context.stage = '图片处理';
        for (let i = 0; i < 60 && !item.ready; i++) {
          notice.textContent = '原图已上传，等待后台生成图片…';
          await new Promise(resolve => setTimeout(resolve, 1000));
          if (!root!.isConnected) return;
          item = await clientGet<Media>(`/admin/media/${item.id}`);
        }
      }
      context.stage = '检查图片就绪';
      if (!item.ready) throw new MediaUploadError('原图已上传，但派生图尚未就绪。请确认后台图片处理服务（worker）已启动，并刷新媒体库查看结果。', 'processing');
      if (!item.variants.length) throw new MediaUploadError('请先在设置中配置媒体公开地址。', 'processing');
      media = [item, ...media.filter(m => m.id !== item.id)].slice(0, 100); render();
      notice.textContent = '图片已就绪，点击媒体复制 Markdown。';
    } catch (error) {
      const report = describeUploadFailure(error, context);
      notice.textContent = report.message; errorDetails.textContent = report.details; errorPanel.hidden = false;
      console.error('[media-upload]', report.details);
    } finally { upload.disabled = false; progress.hidden = true; }
  });
}
