// @ts-check
import { defineConfig } from 'astro/config';
import node from '@astrojs/node';

const devApi = process.env.SLORAY_API_INTERNAL_URLS?.split(',')[0]?.trim()
  || 'http://127.0.0.1:8080';

/**
 * 渲染策略：**SSR 为主**，而不是「发布即重建」。
 *
 * 博客是"动态"的（评论、计数、搜索、后台写作都要实时），
 * 所以文章发布后必须立刻可见，不能等一次 CI 构建。
 * 代价是每个页面请求都要经过 Node 进程，因此读路径靠 HTTP 缓存兜底
 * （见页面里的 Cache-Control 设置与 deploy/nginx/sloray.conf）。
 *
 * `output: 'server'` 让所有页面默认 SSR；少数纯静态页
 * （404 等）用 `export const prerender = true` 单独静态化；普通页面由后台维护。
 */
export default defineConfig({
  output: 'server',
  adapter: node({ mode: 'standalone' }),
  integrations: [{
    name: 'sloray:standalone-build',
    hooks: {
      'astro:config:setup': ({ command, updateConfig }) => {
        // 发布包只带 dist；dev 需由 Node 原生加载 CommonJS，不能全量 noExternal。
        if (command === 'build') updateConfig({ vite: { ssr: { noExternal: true } } });
      },
    },
  }],
  vite: {
    server: { proxy: {
      '/api': devApi, '/login': devApi, '/logout': devApi, '/setup': devApi,
      '/admin/settings': devApi, '/healthz': devApi, '/readyz': devApi,
    } },
  },

  // canonical / sitemap 用的站点地址（生产环境务必设置）。
  site: process.env.SLORAY_PUBLIC_SITE_URL || 'http://localhost:4321',

  server: {
    // 只监听回环：公网流量一律经 Nginx 进入，Astro 不直接暴露。
    host: '127.0.0.1',
    port: Number(process.env.PORT ?? 4321),
  },

  // 局部导航（pjax 式体验）由内置 ClientRouter 提供，见 BaseLayout.astro。
  // 不引入第三方 pjax 库：两者会争抢 history 与滚动位置。

  /**
   * 关掉 Astro 内置的 session。
   *
   * Astro 7 在 @astrojs/node 下会**默认启用文件系统 session 存储**
   * （构建日志里会出现 "Enabling sessions with filesystem storage"）。
   * 那等于把状态写在本地磁盘上，与"进程无状态、随时可以多起一个实例"
   * 直接冲突，而且不会有人注意到 —— 直到水平扩展时登录态开始随机失效。
   *
   * 登录态走我们自己的 Postgres `sessions` 表（见 ADR-0007）。
   */
  session: false,
});
