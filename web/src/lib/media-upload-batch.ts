export type UploadState = 'queued' | 'uploading' | 'processing' | 'ready' | 'pending' | 'failed';

export interface UploadEntry<T> {
  file: File;
  state: UploadState;
  progress: number;
  media?: T;
  error?: unknown;
}

interface UploadOperations<T> {
  transfer: (file: File, progress: (percent: number) => void) => Promise<T>;
  poll: (item: T) => Promise<T>;
  ready: (item: T) => boolean;
  onMedia: (item: T) => void;
  onChange: (entries: readonly UploadEntry<T>[]) => void;
  active?: () => boolean;
  wait?: () => Promise<void>;
  attempts?: number;
}

/** Register every original before waiting for derivatives, and keep registered items on polling failure. */
export async function runMediaUploadBatch<T>(files: readonly File[], operations: UploadOperations<T>) {
  const entries: UploadEntry<T>[] = files.map(file => ({ file, state: 'queued', progress: 0 }));
  const active = operations.active ?? (() => true);
  const wait = operations.wait ?? (() => new Promise<void>(resolve => setTimeout(resolve, 1000)));
  const changed = () => { if (active()) operations.onChange(entries); };
  changed();
  for (const entry of entries) {
    if (!active()) return entries;
    entry.state = 'uploading'; changed();
    try {
      entry.media = await operations.transfer(entry.file, percent => {
        entry.progress = Math.max(0, Math.min(100, percent)); changed();
      });
      entry.progress = 100;
      entry.state = operations.ready(entry.media) ? 'ready' : 'processing';
      if (active()) operations.onMedia(entry.media);
    } catch (error) {
      entry.state = 'failed'; entry.error = error;
    }
    changed();
  }
  for (let attempt = 0; attempt < (operations.attempts ?? 60); attempt++) {
    const processing = entries.filter(entry => entry.state === 'processing');
    if (!processing.length || !active()) return entries;
    await wait();
    for (const entry of processing) {
      if (!active()) return entries;
      try {
        entry.media = await operations.poll(entry.media!);
        entry.state = operations.ready(entry.media) ? 'ready' : 'processing';
        if (active()) operations.onMedia(entry.media);
      } catch (error) {
        entry.state = 'pending'; entry.error = error;
      }
      changed();
    }
  }
  for (const entry of entries) if (entry.state === 'processing') entry.state = 'pending';
  changed();
  return entries;
}
