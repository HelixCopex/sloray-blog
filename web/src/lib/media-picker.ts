import { mediaMarkdown } from './media-markdown.mjs';
import { clientGet, clientWrite } from './api';
import { describeUploadFailure, MediaUploadError, uploadOriginal, type UploadContext } from './media-upload';
import { runMediaUploadBatch, type UploadEntry } from './media-upload-batch';
import type { components } from './schema';
import { notify } from './toast';
type Media = components['schemas']['Media'];
type Presigned = components['schemas']['Presigned'];

export function bindMediaPicker() {
  const root = document.querySelector<HTMLElement>('[data-media-picker]');
  if (!root || root.dataset.bound) return;
  root.dataset.bound = 'true';
  const dialog = root.closest<HTMLDialogElement>('dialog');
  const list = root.querySelector<HTMLElement>('[data-media-list]')!;
  const input = root.querySelector<HTMLInputElement>('#image-file')!;
  const upload = root.querySelector<HTMLButtonElement>('[data-upload]')!;
  const uploadLabel = root.querySelector<HTMLElement>('[data-upload-label]')!;
  const selection = root.querySelector<HTMLElement>('[data-file-selection]')!;
  const uploadStatus = root.querySelector<HTMLElement>('[data-upload-status]')!;
  const uploadList = root.querySelector<HTMLUListElement>('[data-upload-list]')!;
  const refresh = root.querySelector<HTMLButtonElement>('[data-refresh-media]')!;
  const progress = root.querySelector<HTMLProgressElement>('progress')!;
  const errorPanel = root.querySelector<HTMLDetailsElement>('[data-media-error]')!;
  const errorDetails = root.querySelector<HTMLElement>('[data-media-error-details]')!;
  const alt = root.querySelector<HTMLInputElement>('#image-alt')!;
  const copyField = root.querySelector<HTMLTextAreaElement>('[data-markdown-copy]')!;
  let media: Media[] = [];
  let loadVersion = 0;
  let retryFiles: File[] | undefined;
  const deletedIds = new Set<string>();
  const mediaNodes = new Map<string, {
    card: HTMLLIElement;
    visual: HTMLElement;
    description: HTMLParagraphElement;
    actions: HTMLDivElement;
    image?: HTMLImageElement;
  }>();
  const button = (label: string, action: string, id: string) => {
    const element = document.createElement('button');
    element.type = 'button'; element.textContent = label; element.dataset.action = action; element.dataset.id = id;
    return element;
  };
  function render() {
    const focused = list.contains(document.activeElement) ? document.activeElement as HTMLButtonElement : undefined;
    const retained = new Set(media.map(item => item.id));
    for (const [id, node] of mediaNodes) {
      if (!retained.has(id)) { node.card.remove(); mediaNodes.delete(id); }
    }
    list.querySelector('.media-empty')?.remove();
    let cursor = list.firstChild;
    for (const item of media) {
      let node = mediaNodes.get(item.id);
      if (!node) {
        const card = document.createElement('li'); card.className = 'media-item'; card.dataset.id = item.id;
        const visual = document.createElement('div');
        const description = document.createElement('p'); description.className = 'hint';
        const actions = document.createElement('div'); actions.className = 'media-item-actions';
        card.append(visual, description, actions);
        node = { card, visual, description, actions }; mediaNodes.set(item.id, node);
      }
      const image = item.variants.at(-1);
      if (item.ready && image) {
        if (!node.image) {
          const copy = button('', 'copy', item.id); copy.className = 'media-image'; copy.setAttribute('aria-label', '复制图片 Markdown');
          const frame = document.createElement('focus-image'); frame.className = 'image-frame';
          node.image = document.createElement('img'); node.image.alt = ''; node.image.loading = 'lazy';
          frame.append(node.image); copy.append(frame); node.visual.replaceWith(copy); node.visual = copy;
        }
        if (node.image.getAttribute('src') !== image.url) node.image.src = image.url;
        if (item.width && item.height) {
          if (node.image.getAttribute('width') !== String(item.width)) node.image.width = item.width;
          if (node.image.getAttribute('height') !== String(item.height)) node.image.height = item.height;
        }
      } else if (!node.visual.classList.contains('media-pending')) {
        const pending = document.createElement('div'); pending.className = 'media-image media-pending'; pending.textContent = '处理中';
        pending.setAttribute('aria-label', '图片已上传，等待后台处理'); node.visual.replaceWith(pending); node.visual = pending; node.image = undefined;
      }
      node.description.textContent = `${Math.ceil(item.size_bytes / 1024)} KiB · ${item.ready && image ? `${item.width} × ${item.height}` : '已上传，等待处理'}`;
      for (const [action, label, show] of [
        ['cover', '设为封面', item.ready && image && root!.dataset.allowCover === 'true'],
        ['delete', '删除图片', root!.dataset.manage === 'true'],
      ] as const) {
        const existing = node.actions.querySelector<HTMLButtonElement>(`[data-action="${action}"]`);
        if (!show) existing?.remove();
        else if (!existing) {
          const control = button(label, action, item.id); control.className = action === 'delete' ? 'text danger' : 'text'; node.actions.append(control);
        }
      }
      node.actions.hidden = !node.actions.childElementCount;
      if (node.card !== cursor) list.insertBefore(node.card, cursor);
      cursor = node.card.nextSibling;
    }
    if (!media.length) {
      const empty = document.createElement('li'); empty.className = 'media-empty hint';
      empty.textContent = '媒体库还没有图片，上传后会显示在这里。'; list.append(empty);
    }
    if (focused && document.activeElement !== focused) {
      const replacement = focused.isConnected ? focused : list.querySelector<HTMLButtonElement>(`button[data-id="${focused.dataset.id}"][data-action="${focused.dataset.action}"]`);
      (replacement ?? list.querySelector<HTMLButtonElement>('button[data-action="copy"]') ?? refresh).focus({ preventScroll: true });
    }
  }
  function remember(item: Media) {
    if (!root!.isConnected || deletedIds.has(item.id)) return;
    const index = media.findIndex(previous => previous.id === item.id);
    const previous = media[index];
    const changed = !previous || previous.ready !== item.ready || previous.width !== item.width || previous.height !== item.height
      || previous.variants.length !== item.variants.length || previous.variants.some((variant, variantIndex) => variant.url !== item.variants[variantIndex]?.url);
    if (index < 0) media = [item, ...media].slice(0, 100);
    else media[index] = item;
    if (changed) render();
  }
  async function load() {
    if (input.disabled) return;
    const version = ++loadVersion;
    refresh.disabled = true;
    list.setAttribute('aria-busy', 'true');
    try {
      const loaded = await clientGet<Media[]>('/admin/media');
      if (!root!.isConnected || version !== loadVersion) return;
      media = loaded.filter(item => !deletedIds.has(item.id));
      render();
    } catch (error) { notify(error instanceof Error ? error.message : '加载失败，请刷新重试。', 'error'); }
    finally { if (version === loadVersion) { refresh.disabled = false; list.removeAttribute('aria-busy'); } }
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
      copyField.value = mediaMarkdown(alt.value, image.url, { width: item.width, height: item.height });
      root.querySelector<HTMLElement>('[data-copy-result]')!.hidden = false;
      try { await navigator.clipboard.writeText(copyField.value); notify('Markdown 图片链接已复制，返回正文粘贴即可。', 'success'); }
      catch {
        copyField.focus(); copyField.select();
        notify('浏览器未允许自动复制，请复制下方已选中的 Markdown 链接。', 'warning');
      }
    } else if (target.dataset.action === 'cover' && image) {
      const cover = document.querySelector<HTMLInputElement>('#cover-media');
      if (!cover) return;
      cover.value = item.id; cover.dispatchEvent(new Event('input', { bubbles: true }));
      const preview = document.querySelector<HTMLImageElement>('[data-cover-preview]')!;
      if (item.width && item.height) { preview.width = item.width; preview.height = item.height; }
      preview.src = image.url; preview.closest<HTMLElement>('focus-image')!.hidden = false;
      document.querySelector<HTMLElement>('[data-cover-status]')!.textContent = '已选择封面。';
      notify('封面已选择，保存文章后生效。', 'success');
      document.querySelector<HTMLButtonElement>('[data-clear-cover]')!.hidden = false;
      dialog?.close();
    } else if (target.dataset.action === 'delete') {
      if (!confirm('删除图片并移除使用它的文章封面？正文中的旧链接需要手动移除。')) return;
      target.disabled = true;
      try { await clientWrite(`/admin/media/${item.id}`, 'DELETE', {}); deletedIds.add(item.id); media = media.filter(m => m.id !== item.id); render(); notify('图片已删除。', 'success'); }
      catch (error) { notify(String(error), 'error'); target.disabled = false; }
    }
  });
  function selectedFiles() { return retryFiles ?? Array.from(input.files ?? []); }
  function updateSelection() {
    const files = selectedFiles();
    selection.textContent = files.length
      ? `${retryFiles ? '待重试' : '已选择'} ${files.length} 张 · ${(files.reduce((size, file) => size + file.size, 0) / 1024 / 1024).toFixed(1)} MiB`
      : '尚未选择图片';
    uploadLabel.textContent = files.length ? `${retryFiles ? '重试失败的' : '上传'} ${files.length} 张图片` : '上传图片';
    upload.disabled = !files.length;
  }
  input.addEventListener('change', () => {
    retryFiles = undefined; updateSelection(); uploadList.hidden = true; uploadStatus.hidden = true;
    errorPanel.hidden = true; errorPanel.open = false;
  });
  updateSelection();
  upload.addEventListener('click', async () => {
    const files = selectedFiles();
    if (!files.length || input.disabled) return;
    errorPanel.hidden = true; errorPanel.open = false; errorDetails.textContent = '';
    loadVersion++; list.removeAttribute('aria-busy'); refresh.disabled = true;
    upload.disabled = true; input.disabled = true; progress.value = 0; progress.hidden = false;
    uploadLabel.textContent = '正在上传…';
    uploadList.replaceChildren(); uploadList.hidden = false; uploadStatus.hidden = false;
    const contexts = new Map<File, UploadContext>();
    const rows = files.map(file => {
      const row = document.createElement('li');
      const name = document.createElement('span'); name.className = 'media-upload-name'; name.textContent = file.name;
      const state = document.createElement('span'); state.className = 'hint';
      row.append(name, state); uploadList.append(row); return { row, state };
    });
    function updateBatch(entries: readonly UploadEntry<Media>[]) {
      let uploaded = 0;
      for (const [index, entry] of entries.entries()) {
        const context = contexts.get(entry.file);
        const labels = { queued: '等待上传', uploading: context?.stage ?? '校验图片', processing: '已上传，正在处理', ready: '已就绪', pending: '已上传，等待处理', failed: '上传失败' };
        rows[index].row.dataset.state = entry.state;
        rows[index].state.textContent = entry.state === 'uploading' && context?.stage === '原图上传'
          ? `正在上传 ${Math.round(entry.progress)}%` : labels[entry.state];
        uploaded += entry.state === 'queued' ? 0 : entry.state === 'uploading' ? entry.progress / 100 : 1;
      }
      progress.value = uploaded / entries.length * 100;
      const transferred = entries.filter(entry => !['queued', 'uploading'].includes(entry.state)).length;
      const failed = entries.filter(entry => entry.state === 'failed').length;
      uploadStatus.textContent = transferred === entries.length
        ? `已检查 ${entries.length} 张图片，${entries.filter(entry => entry.state === 'ready').length} 张已就绪${failed ? `，${failed} 张上传失败` : ''}${entries.some(entry => entry.state === 'processing') ? '，正在等待后台处理。' : '。'}`
        : `正在上传第 ${transferred + 1} / ${entries.length} 张图片。`;
    }
    try {
      const entries = await runMediaUploadBatch<Media>(files, {
        active: () => root.isConnected,
        onChange: updateBatch,
        onMedia: remember,
        ready: item => item.ready && item.variants.length > 0,
        transfer: async (file, onProgress) => {
          const context: UploadContext = { stage: '校验图片', origin: window.location.origin, contentType: file.type, sizeBytes: file.size };
          contexts.set(file, context);
          if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) throw new MediaUploadError('请选择 PNG、JPEG 或 WebP 图片。', 'validation');
          if (file.size > 10 * 1024 * 1024) throw new MediaUploadError('图片超过 10 MiB。', 'validation');
          if (!file.size) throw new MediaUploadError('图片文件为空，请重新选择。', 'validation');
          const sha256 = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer())), b => b.toString(16).padStart(2, '0')).join('');
          context.stage = '申请上传许可'; onProgress(0);
          const signed = await clientWrite<Presigned>('/admin/media/presign', 'POST', { content_type: file.type, size_bytes: file.size, sha256 });
          context.uploadId = signed.upload_id ?? undefined; context.uploadUrl = signed.upload_url ?? undefined; context.headers = signed.headers;
          if (signed.media) { context.stage = '图片处理'; return signed.media; }
          context.stage = '原图上传'; onProgress(0);
          await uploadOriginal(signed.upload_url!, file, signed.headers, onProgress);
          context.stage = '登记媒体'; onProgress(100);
          const item = await clientWrite<Media>('/admin/media', 'POST', { upload_id: signed.upload_id });
          context.stage = '图片处理'; return item;
        },
        poll: async item => {
          const updated = await clientGet<Media>(`/admin/media/${item.id}`);
          if (updated.ready && !updated.variants.length) throw new MediaUploadError('请先在设置中配置媒体公开地址。', 'processing');
          return updated;
        },
      });
      if (!root.isConnected) return;
      const failed = entries.filter(entry => entry.state === 'failed');
      const pending = entries.filter(entry => entry.state === 'pending');
      const ready = entries.filter(entry => entry.state === 'ready');
      const reports = entries.flatMap(entry => {
        const context = contexts.get(entry.file)!;
        const error = entry.error ?? (entry.state === 'pending' ? new MediaUploadError('原图已上传，但派生图尚未就绪。请确认后台图片处理服务（worker）已启动，并刷新媒体库查看结果。', 'processing') : undefined);
        if (!error) return [];
        const report = describeUploadFailure(error, context);
        const details = `图片：${entry.file.name}\n${report.message}\n${report.details}`;
        console.error('[media-upload]', details); return [details];
      });
      errorDetails.textContent = reports.join('\n\n'); errorPanel.hidden = !reports.length;
      const summary = `${ready.length} 张已就绪${pending.length ? `，${pending.length} 张已上传、等待处理` : ''}${failed.length ? `，${failed.length} 张上传失败` : ''}。`;
      uploadStatus.textContent = summary;
      notify(summary, failed.length ? 'error' : pending.length ? 'warning' : 'success', { id: 'media-upload' });
      retryFiles = failed.length ? failed.map(entry => entry.file) : undefined;
      if (!failed.length) input.value = '';
    } finally {
      input.disabled = false; refresh.disabled = false; progress.hidden = true; updateSelection();
    }
  });
}
