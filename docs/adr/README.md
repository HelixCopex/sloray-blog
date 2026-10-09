# 架构决策记录（ADR）

每一条都记录**当时为什么这么选**。改了主意就新写一条 ADR 并在旧的上标
`Superseded by ADR-XXXX`，不要直接改历史。

| 编号 | 决策 | 状态 |
|---|---|---|
| [0001](0001-two-deployables.md) | 只保留两个可部署单元，不拆微服务 | 已接受 |
| [0002](0002-same-origin-routing.md) | 单域名 + 反向代理路径分流，而不是跨域 CORS | 已接受 |
| [0003](0003-ssr-with-http-caching.md) | 文章页走 SSR + HTTP 缓存，而不是发布即重建 | 已接受 |
| [0004](0004-clientrouter-over-pjax.md) | 局部导航用 Astro ClientRouter，不手写 pjax | 已接受 |
| [0005](0005-postgres-single-source-of-truth.md) | Postgres 唯一权威源，派生数据必须可重建 | 已接受 |
| [0006](0006-raw-pg-ddl-migrations.md) | 迁移用原生 Postgres DDL，且与 API 启动解耦 | 已接受 |
| [0007](0007-database-sessions.md) | 会话存数据库表，而不是 JWT | 已接受 |
| [0008](0008-no-redis-yet.md) | 暂不引入 Redis，并写明触发条件 | 已接受 |
| [0009](0009-postgres-fts-over-meilisearch.md) | 中文检索用 Postgres FTS + pg_jieba，不上 Meilisearch | 已接受 |
| [0010](0010-object-storage-presigned-upload.md) | 媒体用对象存储 + 浏览器预签名直传 | 已接受 |
| [0011](0011-build-on-vps.md) | 在 VPS 上编译部署，CI 只做质量门 | 已接受 |
| [0012](0012-setup-wizard-and-single-binary.md) | Web 安装向导 + 单一二进制 + 内置 systemd 单元 | 已接受 |
| [0013](0013-nginx-reverse-proxy.md) | 目标部署使用 Nginx，同源分流与被动故障切换 | 已接受 |
| [0014](0014-content-contract-and-ssr-build.md) | 内容业务边界、接口契约、SSR 依赖打包与缓存重验证 | 已接受 |
| [0015](0015-background-work-and-shared-limits.md) | 数据库共享限流与单二进制后台任务 | 已接受 |
| [0016](0016-search-extension-fallback.md) | 中文扩展探测与应用分词兜底 | 已接受 |
| [0017](0017-media-pipeline.md) | 媒体流水线与可恢复对象操作 | 已接受 |
| [0018](0018-private-and-public-media-buckets.md) | 原图与公开派生图使用不同 bucket | 已接受 |

配套文档：[deployment.md](../deployment.md)（部署手册）、
[search.md](../search.md)（检索设计）、[media.md](../media.md)（媒体设计）。
