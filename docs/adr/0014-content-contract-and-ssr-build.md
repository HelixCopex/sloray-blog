# ADR-0014：内容业务边界与可复现 SSR 契约

* 状态：已接受
* 日期：2026-10-09

## 约束

M1/M2 的文章业务开始同时被公共读接口、后台写作和未来的 worker 调用。
把授权、修订事务和版本冲突放在 HTTP handler 会产生重复规则。
AGENTS.md 2.2 的稳定职责和跨消费者复用条件已满足。

## 决策

新增 `sloray-content`，只依赖 store。web 编排 HTTP，content 维护内容业务，
store 保持数据库结构职责。DTO 用 utoipa 生成 OpenAPI，前端从契约生成类型。
生成文件纳入仓库，并由 CI 校验漂移和向后兼容性。

TypeScript 固定到 5.9.3：Astro 7 的检查工具接受 TS 5/6，
当前 openapi-typescript 7.13 的 peer 约束只接受 TS 5。
采用共同支持的版本，避免覆盖上游 peer 约束；升级前重新验证两套工具。

Markdown 使用 Astro 官方 remark 渲染包与 rehype-sanitize。
Astro 的默认 SSR 打包会把新加的渲染依赖留作外部 import，导致只携带 dist
的发布包无法启动。因此 SSR 显式 noExternal，并验证复制到无 node_modules
的临时发布目录后仍能渲染文章。

## 缓存后果

在 CDN purge 完整适配并验证前，动态文章使用 `public, no-cache`，每次必须重验证。
JSON 与 HTML 各自计算 ETag；HTML 的模板或资源变化也必须使 ETag 变化，不能
把正文 JSON 的强 ETag 当作完整 HTML 的强 ETag。撤回与删除后读接口先检查
可见性，再处理条件请求，不允许旧 ETag 把已撤回文章变成 304。

这暂缓 ADR-0003 的共享缓存 TTL，避免在没有 purge 的前提下承诺失效。
带内容哈希的静态资源继续 immutable。
