# 架构与实现

Rust（axum + SeaORM）负责数据、鉴权与业务；Astro SSR 负责呈现与浏览器交互。
目标是在单台 Debian VPS 上原生部署，并保留横向增加 API 实例的边界。
硬性约定见 [AGENTS.md](../AGENTS.md)，历史决策见 [ADR](adr/README.md)。

## 1. 拓扑与权威数据

```text
浏览器 ── HTTPS ── Nginx ── /api/*、探针、安装/登录/设置 ── sloray serve × 2
                     └── 页面、静态资源 ── Astro SSR ── 内网只读 API
                                                           │
                                                        Postgres
                                                           │
                                              sloray worker --once
                                                           │
                                私有原图 bucket / 公开 WebP bucket / CDN 清理
```

Nginx 是目标机器唯一的反向代理依赖。两个 API 共用 Postgres 会话、限流与配置，
无需粘性会话；Web 是一个 SSR 实例。后台任务由同一个 `sloray` 二进制运行，
systemd timer 每次执行 `worker --once` 并重新加载配置，没有另一套可执行文件。

Postgres 保存文章、修订、评论、用户、会话、上传意图、计数权威记录和 outbox。
媒体原字节的权威副本在私有对象存储，因此数据库备份不能代替原图备份。
搜索向量、聚合计数、WebP 和缓存都可重建。外部对象操作与 CDN 清理走 outbox，
不把数据库提交与外部写入当作一个无法回滚的同步双写。

`serve` 不存文件、会话或队列到本地盘。唯一可写部署目录是 `shared/`，
存放 `config.json` 与 setup token；原图处理在 worker 内存中完成。
Astro session 显式关闭，SSR 发布包自包含，不依赖构建机的 node_modules。

## 2. 职责与依赖

| crate | 职责 | 内部依赖 |
|---|---|---|
| config | 分层配置、校验、原子落盘 | 无 |
| store | SeaORM 实体、连接池 | config |
| auth | 密码、数据库会话、Cookie、用户与共享限流 | store |
| setup | token、数据库探测、迁移调用、嵌入的 systemd 单元 | config、auth、migration |
| content | 文章、标签、修订、评论、搜索、计数与到期发布 | store |
| media | 上传许可、对象校验、派生、清理与 CDN 投递 | config、store |
| web | HTTP 编排、DTO 响应、安装 HTML、日志与服务启动 | 以上各层 |
| migration | PostgreSQL DDL | 无 |
| sloray 根二进制 | CLI 分发 | 各层 |

`routes → 业务 crate → store → Postgres`。store 只描述表；HTTP handler
解析、鉴权、调用业务函数并构造响应。所有公开契约使用 DTO，不暴露实体 Model。
crate 依赖方向、版本引用与 unsafe 禁令由 `check-crates.py` 强制。
Rust 与 Shell 文件每个不超过 500 行，依赖统一由 workspace 管理。

Rust edition 2024，MSRV **1.94.1**（对象存储 SDK 的最低版本）；Node ≥ 22.12，
pnpm 12.9.1，Postgres ≥ 16。锁文件记录实际构建版本。
Rust 网络依赖统一走 rustls；不用 OpenSSL/native-tls，也没有 git 或 `*` 依赖。
宿主 Nginx 的 TLS 实现不受 Rust crate 禁用策略约束。

## 3. 数据模型与一致性

原始 12 张业务表不改历史迁移；新增 expand 迁移增加
`privacy_secrets`、`request_limits`、`search_settings`、`media_uploads`，
另以 expand 迁移增加独立 `pages` 表，再以第六条 expand 迁移增加主题库与启用快照，另以第七条 expand 迁移增加站点标识，第八条 expand 迁移为 pages 增加受控 dock_icon 列，合计 20 张业务表，加 `seaql_migrations` 为 21 张表。

| 数据 | 实现 |
|---|---|
| users / sessions | 邮箱小写、admin/author 角色；Argon2id；会话 token 仅存哈希并可撤销 |
| posts / post_revisions | UUIDv7；版本条件更新；每次编辑与修订在同一事务；删除为软删除 |
| themes / theme_settings | 管理员主题库与独立发布快照、乐观版本、恢复默认和 outbox purge（ADR-0020） |
| site_identity | 站点名称/Favicon、独立内容设置、乐观版本与 outbox purge（ADR-0021） |
| pages | 管理员维护独立页面；草稿/发布、导航显示与排序、版本条件更新和软删除 |
| tags / post_tags | 稳定 slug、文章标签关联；不使用第二个权威索引 |
| comments | 默认 pending；审核后公开；父评论同文章且祖先已通过，最大 5 层 |
| post_view_daily / likes | 复合唯一键作为去重权威；相应聚合计数与记录在同一事务更新 |
| post_stats | 派生聚合，`rebuild-stats` 可从明细重建 |
| media / media_uploads | 到期上传许可；登记幂等；原图完整 hash 经 worker 验证后才公开派生 URL |
| outbox | 投递状态、尝试次数、下次可执行时间；处理成功记录保留 30 天 |
| search_settings / posts.search_* | 已安装中文扩展或应用分词，权威文本变更会使旧应用分词失效 |
| request_limits / privacy_secrets | 数据库固定窗口原子限流；随机盐脱敏，多个 API 共享配额 |

所有时间存 `timestamptz`；数据库与服务器用 UTC，后台定时输入与页面展示使用
北京时间。公开文章必须是 published、未删除且发布时间已到；查询与 ETag 判断
都先重新验证可见性，撤回后不能靠旧 ETag 取得 304。

定时发布每批最多 100 条，用 `FOR UPDATE SKIP LOCKED`，版本只增加一次。
媒体/outbox 每次最多 20 条任务，同样用行锁避开其他 worker；失败指数退避，
上限 900 秒。外部操作要求幂等，数据库提交失败时可以重做，不承诺跨存储 exactly-once。

## 4. 功能与公开契约

文章列表、标签、修订、评论与搜索采用 keyset 游标；默认 20、最多 50 条，
游标绑定筛选范围。搜索排序为 rank / published_at / id；游标还绑定查询、
检索引擎和配置。更新排序键时不能承诺跨多次查询的快照一致性。
用户列表最多 1000 条、媒体管理最多最近 100 张，界面明确说明上限。

后台提供登录、文章草稿/发布/定时发布、Markdown 分栏预览、修订查看、作者创建、
独立页面管理、评论审核、媒体库上传/复用、封面选择与 Markdown 链接复制、媒体删除、运行时设置。
访客导航不显示写作入口；直接访问 `/login` 登录后显示管理后台，后台按角色提供导航。
普通页面从 Postgres 读取，管理员可以发布到根路径并选择导航显示与顺序；系统路径保留。
默认 `/about` 在迁移时从静态内容转为可编辑页面，发布、撤回和改址无需重新构建。
作者只能编辑自己的文章及审核自己文章的评论；用户和媒体管理仅管理员可用。
并发编辑提交旧版本返回 409，前端提示保留当前内容再重新加载。

公开页面提供文章、标签、中文搜索、评论与点赞。评论暂按纯文本显示；
Markdown 正文经 `rehype-sanitize` 清洗，内部标题从 h2 起，文章页只有一个 h1。
文章、独立页面与需登录/CSRF 的 `/admin/preview` 使用同一服务端渲染流程；预览不存正文。
浏览按 UTC 日、文章与脱敏 IP + 有界 User-Agent 去重，避免长期访客 Cookie。
这不是精确独立访客统计：相同 NAT 与浏览器可能合并。
点赞需要登录，以用户/文章唯一键保证重复添加和取消幂等。

`sloray openapi` 与 `/api/v1/openapi.json` 生成相同契约，保存于
`docs/api/openapi.json`。`openapi-typescript` 生成 `web/src/lib/schema.ts`，
客户端封装放在 `api.ts`。CI 检查生成漂移、引用、operationId 唯一性和
相对于 PR 基线的兼容性，包括路径参数、请求/响应引用与认证要求变化。
错误信封固定为 `{"error":{"code":"...","message":"..."}}`；解析拒绝也遵守它，
内部错误不对客户端泄漏 SQL、连接串或凭据。

## 5. SSR、导航与缓存

浏览器使用同源 `/api/v1/*`。SSR 使用回环 API 列表，连接失败或 502/503/504
时对只读请求切换上游；不重放写请求。开发服务器代理 API 与安装/登录/设置，
本地前端的交互无需额外 Nginx。

ClientRouter 管理局部导航、历史与焦点。交互绑定 `astro:page-load`，
用 DOM 标志防止重复监听；后台链接关闭预取，Rust 自渲染页面使用完整导航。
文章/页面编辑器进入与离开采用完整导航边界，使浏览器返回也执行原生未保存检查；
点击站内链接先确认离开，取消时保留正文和预览。
浏览器测试验证导航前后 window 状态保持、历史前进后退与换页后的表单绑定。

文章 JSON 与 HTML 是不同表示，分别计算 ETag。HTML ETag 包含最终渲染结果，
所以评论、作者名、封面、模板和静态资源变化不会继续命中旧 HTML。
正文页使用 `public, no-cache` 与 `Vary: Cookie`，必须重新验证；后台、独立页面、探针、错误与其它动态列表
使用 `no-store`。Nginx 不配置动态代理缓存。只有真实 `_astro` 静态资源长期缓存。

文章发布/编辑/撤回/删除和评论审核写入 `cdn.purge` outbox；当前动态页面
仍要求 CDN 尊重 no-store/no-cache，不能自行设置“Cache Everything”。
设置 Cloudflare zone/token 后 worker 按 URL 清理，失败保留任务退避。
公开 WebP 启用清理时可用 immutable；未启用清理时用 no-cache。
媒体删除立即在 API 隐藏并移除封面，异步删对象及清 CDN，失败可重试。

SSR 从 `/api/v1/site` 读取运行时站点地址，用于 canonical、RSS、sitemap 与 JSON-LD。
构建期站点地址只作为退路；404 也动态读取站点地址和公开主题。

## 6. 安全与配置

Cookie 为 HttpOnly、SameSite=Lax，站点 HTTPS 时加 Secure。
JSON 写请求与 HTML 管理表单均校验 double-submit CSRF；安装依赖一次性 token。
登录、评论、浏览、点赞、创建用户和媒体写入独立限流；数据库不可达时失败关闭。
只在 loopback 代理连接上信任 X-Real-IP，Nginx 必须覆盖客户端同名头。

密码计算与图片解码/缩放放在阻塞线程；图片限制为 10 MiB、10000px 边长、
3200 万像素与 128 MiB 解码分配。只收 PNG/JPEG/WebP，拒绝 SVG 和伪装类型。
原图 bucket 关闭公开访问；公开派生 bucket 必须不同，详见 [media.md](media.md)。

配置顺序为默认值 → JSON → 环境变量。验证与磁盘原子写入成功后才切换配置与连接池。
JSON 和 token 文件为 0600，共享目录为 0700；秘密不回显、不入库。
另一个实例每 2 秒读取配置，自己的监听端口不会被共享配置覆盖。
未配置或暂时无法连接数据库时 serve 可以启动；损坏的 JSON 是启动错误，需先
保留副本并修复文件。token CLI 可独立于损坏配置执行。

## 7. 部署、回滚与恢复

[部署手册](deployment.md) 描述 Debian 初始化、Nginx TLS、版本目录与 systemd。
构建使用锁文件。原子切换 current 之前校验版本与 SHA-256，拒绝覆盖现有版本。
构建和激活分别有锁，拒绝路径穿越、保留目录删除及覆盖未提交源码。
激活保存旧单元，停止 worker 后迁移、逐个重启 API、重启 Web，最后恢复 timer。
失败恢复旧 symlink 与服务单元。首次无配置检查 healthz，已配置检查 readyz。

迁移使用事务级咨询锁；serve 不在每次启动时执行迁移。回滚二进制不回滚 schema，
必须 expand → 切换 → 下一版本 contract。生产不能用 fresh 或 down 做发布回滚。
Web 单实例重启有短暂失败窗口，Nginx 被动切换也不保证所有写请求零失败。

逻辑备份带 SHA-256，配置远端时只有上传成功才清理旧备份。
`pg_restore --list` 只检查目录，恢复演练比较全部业务表。
PITR 另需物理基础备份和完整连续 WAL；隔离演练验证恢复目标前保留、目标后丢弃。
操作边界与季度演练见 [backup.md](backup.md)。

## 8. 里程碑与验证

| 阶段 | 内容 | 状态 |
|---|---|---|
| M0 | 分层、配置、日志、错误、探针、迁移、Astro、Nginx/systemd/CI | 已实现 |
| M1 | 内容读路径、契约、SSR、SEO、缓存重验证与 purge 投递 | 已实现 |
| M2 | 登录、后台写作、修订、乐观锁、直传与派生图 | 已实现 |
| M3 | 评论审核、回复、限流、蜜罐 | 已实现 |
| M4 | 中文 FTS、触发器、GIN、SearchIndex 接缝与重建 | 已实现 |
| M5 | 浏览去重、幂等点赞与聚合重建 | 已实现 |
| M6 | 备份/PITR 演练、搜索压测、浏览器 a11y/SEO 与 purge 重试检查 | 已实现并完成本地验证，记录见 verification.md |

测试分为 Rust 单元/HTTP 测试、真实 PG 集成、真实 Chromium 交互与 axe、
真实 Nginx 路径分流，以及隔离的发布/回滚与备份恢复。
CI 使用 PostgreSQL 18 与匹配版本客户端，另有隔离 PITR job；
实际本地执行的版本、输出与外部环境限制见 [验证记录](verification.md)。

明确不做微服务、消息中间件、K8s、Redis、独立搜索引擎或 API 本地文件存储。
没有在真实 Debian VPS、线上 R2/Cloudflare 或远程 GitHub Actions 上执行的验证，
不能由本地通过推断。原生扩展兼容性依赖宿主，应用分词是可用的无扩展退路。

主题包、原生交互组件与服务端群岛的边界见 [ADR-0020](adr/0020-data-themes-and-astro-islands.md) 和 [主题手册](themes.md)。

响应式 Dock、首页线性区块与可刷新组件域见 [ADR-0021](adr/0021-responsive-dock-and-home-composition.md)。
纯图标 Dock、SSR 随动目录与主题背景参数见 [ADR-0022](adr/0022-icon-dock-following-toc-and-background.md)。

页面上下文 Dock 与弹窗架构见 [ADR-0023](adr/0023-contextual-dock-and-action-dialogs.md)。
SSR 负责提供资源身份与可见操作，持久 Dock 以稳定动作键同步新页面，保留原有按钮节点与动画。
搜索/评论使用原生 dialog；文章主要操作与后台编辑主要操作集中于 Dock。
异步统计/点赞通过页面代次隔离，旧文章结果不能覆盖新文章状态；业务权限、审核、CSRF 与版本锁继续由 Rust 执行。

MD3 切页和图片内容属性见 [ADR-0024](adr/0024-material-motion-and-content-image-dimensions.md)。
图片宽高在 body_md 中保存并随修订保留；共用 Markdown AST 解析属性，清洗后控件按原始比例预留空间。
浏览器在保存前读取外部图片缺失尺寸，SSR 不访问任意图片 URL。图片解码只改变绘制状态，切页仅使用位移和透明度。

后台持久导航与统一操作通知见 [ADR-0025](adr/0025-persistent-admin-chrome-and-operation-toasts.md)。
Tab 位于正文动效外；文章进度轨道透明，独立页面不渲染进度。
Astro 和 Rust 页面共享原生 Toast，完整导航后的反馈只使用一次性浏览器消息。
图片保存优先读取媒体元数据，单张远端图片尺寸失败不阻断保存，并以警告 Toast 提醒补全。

切页绘制按 [ADR-0026](adr/0026-sequential-live-content-motion.md) 使用真实 main 元素：
加载成功后先完整滑出淡出，再由 ClientRouter 替换正文并滑入淡入，MD3 曲线且无缩放。
before-swap 跳过浏览器快照，动态群岛按实际布局绘制；动画以 both 保持交接端点。
中止导航和减少动态效果会恢复可见样式，旧导航回调不能取消新导航的动画。
