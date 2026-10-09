import './toast-runtime.js';
export type ToastKind = 'info' | 'success' | 'warning' | 'error';
export function notify(message: string, kind: ToastKind = 'info', options: { id?: string; persist?: boolean } = {}) {
  window.dispatchEvent(new CustomEvent('sloray:toast', { detail: { message, kind, ...options } }));
}
