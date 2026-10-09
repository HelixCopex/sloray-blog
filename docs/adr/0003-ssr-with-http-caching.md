# ADR-0003：文章页走 SSR + HTTP 缓存，而不是「发布即重建」

**状态**：已接受

缓存 TTL 部分 Superseded by [ADR-0014](0014-content-contract-and-ssr-build.md)；SSR 决策保持。

## 背景

Astro 的默认心智模型是静态站点生成（SSG）：构建期把页面渲染成 HTML。
内容型站点通常这么做，因为产物可以直接丢 CDN。

但这个项目的定位是**动态博客**：评论、浏览计数、点赞、全文搜索、
后台写作都需要实时数据。

## 决策

* Astro 使用 `output: 'server'`（`@astrojs/node` standalone），页面默认 SSR。
* 少数纯静态页（`/about`、404）用 `export const prerender = true` 单独静态化。
* 文章页的读路径靠 **HTTP 缓存**兜底，而不是靠预渲染：

  ```
  Cache-Control: public, s-maxage=60, stale-while-revalidate=600
  ```

  并透传 API 返回的 `ETag` / `If-None-Match`。

## 理由

1. **发布的即时性。** SSG 要求「发布文章 → 触发重建 → 等构建 → 部署」。
   在单机部署下这条链路本身就是停机风险，且随文章数增长而变慢。
   SSR 让发布在数据库提交的瞬间生效。
2. **动态部分无法静态化。** 评论与计数出现在文章页上，SSG 只能把它们
   降级成客户端水合，反而更慢、更糟。
3. **缓存能覆盖绝大部分读流量。** 博客的读模式是极度倾斜的：
   少数文章占据多数访问。`s-maxage` + `stale-while-revalidate`
   让 CDN/Caddy 承担这部分，Node 进程只处理未命中。
4. **`prerender` 保留了静态化的能力。** 真的需要极致性能的页面，
   可以逐页开启，不必全局切换渲染模式。

## 影响

* 部署需要常驻 Node 进程（systemd 单元 `sloray-web.service`）。
* **缓存失效成为必须处理的问题**：文章撤回/删除后不能还挂着缓存的 HTML。
  策略是先靠短 `s-maxage` 兜底，并在 M1 引入显式 purge（surrogate key 或
  按标签失效）。见 `docs/architecture.md` 的失败模式一节。
* Web 实例重启会有秒级窗口。若需要 Web 也零停机，
  按 `sloray@.service.in` 的样子起两个实例。
