/// <reference types="astro/client" />

/**
 * 只有带 `SLORAY_` 前缀（非 `PUBLIC_`）的变量才是**仅服务端**可见的。
 * 这样 API 内网地址不会被泄漏进客户端 bundle。
 */
interface ImportMetaEnv {
  readonly SLORAY_API_INTERNAL_URL?: string;
  readonly SLORAY_PUBLIC_SITE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare namespace App { interface Locals { publicSiteUrl?: string; siteIdentity?: import('./lib/schema').components['schemas']['SiteIdentity']; theme?: import('./lib/schema').components['schemas']['ThemePackage'] } }
