import type { components } from './schema';
type ThemePackage = components['schemas']['ThemePackage'];
export const defaultTheme: ThemePackage;
export function cssForStyle(css: string): string;
export function themeCss(theme: ThemePackage): string;
export function escapeHtml(value: unknown): string;
export function previewHtml(theme: ThemePackage, styles: string, appearance?: string, icons?: string): string;

export const widgetLabels: Record<string, string>;
export function homeRows(theme: ThemePackage): (import('./schema').components['schemas']['HomeRow'] & { widgets: import('./schema').components['schemas']['HomeWidget'][] })[];
