# 本地验证记录

日期：2026-10-09（Asia/Shanghai）。以下是本次实际执行结果，不表示远程 CI 或目标 VPS 已部署。
环境：Rust stable 1.98.1，以及实际安装验证的 MSRV 1.94.1；Node 24.19.0、pnpm 12.9.1、
PostgreSQL 18.6；Nginx 1.30.5、ShellCheck 0.11.0、actionlint 1.7.12；
Playwright 1.62.1 对应 Chromium 151.0.7922.34，axe-core 4.11.1。
系统 Node 20 不满足要求，因此本次前端使用独立 Node 24。

## 1. 贡献指南门禁

实际执行：

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/check-architecture.sh
cd web
pnpm run api:check
pnpm run check
pnpm run build
```

fmt 退出 0，无输出；clippy 退出 0，最后输出：

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.20s
```

Rust 单元/HTTP 测试合计 **53 passed / 0 failed**。各非空测试组实际输出摘录：

```text
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

架构与依赖门禁输出：

```text
== 单文件行数上限：500 行（*.rs 与 *.sh） ==
  ✓ 全部合规
advisories ok, bans ok, licenses ok, sources ok
  ✓ advisories / bans / licenses / sources 全部通过
  ✓ crate 依赖方向 / workspace 版本引用 / unsafe 禁令通过
结果：全部通过
```

另以独立目标目录实际验证最低 Rust 版本：

```bash
CARGO_TARGET_DIR=target/msrv cargo +1.94.1 check --locked --offline --workspace --all-targets
```

```text
Checking sloray v0.1.0
Finished `dev` profile [unoptimized + debuginfo] target(s) in 39.11s
```

OpenAPI 生成文件与 Rust 输出逐字节相同；TS api:check 退出 0。astro check：

```text
Result (27 files):
- 0 errors
- 0 warnings
- 0 hints
```

生产 build 成功，额外以首次发布的空 SLORAY_PUBLIC_SITE_URL 构建也成功：

```text
[build] Server built in 1.01s
[build] Complete!
```

build 仍有 **5 条第三方空字符串 MODULE_LEVEL_DIRECTIVE 警告**，来自 MDX/hast 的打包。
这是实际存在的构建警告，未屏蔽，不能把 build 报告为“零警告”。发布包移至无 node_modules
目录后，SSR/Markdown/SEO 与浏览器流程通过。

## 2. 真实 Postgres 集成与恢复

使用回环临时 PG；每个测试创建 UUID 命名 UTF8 数据库并在结束删除，不使用生产库。
运行命令（连接串仅为本地示例）：

```bash
export SLORAY_TEST_DATABASE_URL=postgres://sloray@127.0.0.1:55432/sloray
python3 scripts/smoke-setup.py
python3 scripts/smoke-migrations.py
python3 scripts/smoke-content.py
python3 scripts/smoke-activity.py
python3 scripts/smoke-search.py
python3 scripts/smoke-media.py
python3 scripts/smoke-backup.py
python3 scripts/smoke-pitr.py
```

实际输出摘录：

```text
PASS 损坏配置下 setup-token 可用；16 个并发创建共享一个私有 token
PASS 仅配置连接串但无管理员时 setup 可用；两个实例使用各自 --bind 端口
PASS 并发 setup 只创建一位管理员；共享配置后两实例自动就绪并关闭 setup
PASS 登录会话跨实例可用；配置以 0600 写入，扁平 S3 环境变量正确映射
PASS 四条迁移 Applied；down all → up、down 1 → up、fresh，17 张表重建成功
PASS 文章创建/发布/标签/修订，未登录和缺少 CSRF 拒绝写入，旧版本更新返回 409
PASS 会话与作者权限，公共详情支持 ETag/304
PASS 无 node_modules 的 SSR 发布包、内网 API 故障切换、Markdown 清洗、SEO、后台登录和 HTML ETag
PASS 撤回后旧 ETag 不会返回 304；软删除后后台和公开接口不可见
PASS 评论先审后发、蜜罐丢弃、回复校验、CSRF 和共享数据库限流
PASS 24 次并发浏览仅计一次，重复点赞/取消幂等，计数可从权威记录重建
PASS 两个 worker 只发布一次；HTML 登录缺少 CSRF 被拒绝
PASS 中文 FTS，草稿排除、同排序值 keyset 分页、游标范围校验、OR 兜底和排除语法
PASS SQL 绕过应用修改使旧向量失效；worker 从原文修复后中文子词可查
PASS 1007 篇文章 / 8 并发 / 100 请求：搜索 P95=9.91ms，中位数=4.81ms（本机临时 PG）
PASS 预签名直传、条件创建、防重放覆盖、两阶段登记、私有原图与验证后的 SHA-256 秒传
PASS worker 生成 WebP 派生图，元数据标记消失，登记重试幂等
PASS magic bytes 拒绝伪装 SVG，删除立即隐藏，CDN 失败退避/重试，孤儿 GC 默认 dry-run
PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，17 张表内容与迁移记录逐表一致
PASS 物理基础备份 + WAL 归档 + recovery_target_time 恢复并提升；保留目标前记录，排除目标后记录
```

媒体测试仅连接本机 S3/CDN 协议模拟器；验证私有 bucket 无匿名读取、公开 bucket 无原图，
完整 SHA-256 校验后才公开 URL。故意制造 CDN 500，日志出现一次预期 WARN，随后重试成功。
检索压力测试为本机小数据集、应用分词模式，100 请求量，不能代表目标 VPS 容量或生产 SLO。
pg_jieba 上游递归子模块 + CMake 在 PG 18 开发头文件下编译成功；未安装到宿主或验证线上扩展。

## 3. 浏览器、a11y 与 SEO

```bash
cd web && pnpm exec playwright install chromium
# 回到仓库根，并保持上述 SLORAY_TEST_DATABASE_URL
python3 scripts/smoke-browser.py
```

临时同源代理连接真实 API、SSR 和本地对象模拟器，不访问线上账户。
Chromium 实际输出：

```text
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

检查包括 window 状态跨局部导航保持、历史前进后退、动态表单绑定、真实 Cookie/CSRF、
直接 PUT、封面实际加载、连续保存版本 2、审核后公开、删除封面消失、无 pageerror。
axe 标签 wcag2a/wcag2aa/wcag21aa：所测公开页面与登录/设置/编辑/媒体页未发现违规；
首页还检查深色模式。检查唯一 h1、canonical 和 JSON-LD，RSS/sitemap 由 SSR 测试核对。
自动检查不替代完整键盘/读屏人工审计或全部页面、屏幕尺寸的验收。

## 4. 部署与 CI 设计

```bash
shellcheck deploy/scripts/*.sh scripts/*.sh
python3 deploy/scripts/test-release-package.py
deploy/scripts/test-activate-release.sh
deploy/scripts/test-rollout.sh
deploy/scripts/test-backup.sh
python3 scripts/test-nginx.py
python3 scripts/test-api-contract.py
python3 scripts/check-api.py --validate
actionlint .github/workflows/ci.yml
```

ShellCheck/actionlint 退出 0，无输出；Shell 全部通过 bash -n。
发布切换/回滚 **23 个 PASS**，滚动流程 **4 个 PASS**，备份策略 **2 个 PASS**。
真实 Nginx 使用临时 TLS 与隔离上游，输出：

```text
PASS 发布包正常路径可接受；拒绝绝对/穿越路径、符号/硬链接、设备与校验清单越界
PASS POST 失败仅投递一次，未重放到另一 API
PASS Nginx 配置、API/安装/登录/设置/SSR 分流、真实 IP 头覆盖
PASS GET 在一个 API 返回 503 时切到健康上游
PASS 全部上游不可用时正确返回失败
PASS OpenAPI 本地引用与唯一 operationId
Ran 6 tests
OK
```

修复/补充的 CI 设计包括：迁移调用统一根二进制；Postgres 16/17/18 使用匹配版本工具；
迁移全量 down/up/fresh，避免残留函数导致 fresh 失败；独立 SSR 包与浏览器交互；
OpenAPI 漂移、引用、operationId、路径参数/请求响应引用/认证兼容性；
部署与 worker 单元回滚；POST 不重放；备份远端失败保留策略与逻辑/PITR 恢复；
Rust MSRV；最小 contents:read 权限、超时及并发取消；锁文件构建；空站点地址首次构建。

没有远程 GitHub Actions 运行结果，本地仅执行相同验证及 actionlint。
该轮验证时仓库尚无 Git 提交，兼容性检查的历史基线需在 PR 上执行；本地 6 个独立契约样例
验证了兼容性检查器，而不是伪造一次历史 diff 通过。

## 5. 尚属外部环境的验证

真实 Debian 初始化、systemd 权限/沙箱和 TLS 续期尚未在目标 VPS 执行。
线上 R2 的签名/CORS/双 bucket 权限、Cloudflare purge 权限和原图异机备份仍需宿主验收。
Postgres 16/17 矩阵已纳入 CI，本次本地使用 18.6，不声称执行了另外两个服务器版本。
没有配置生产季度备份调度、告警、归档保留或测量 VPS 冷构建峰值。
历史 ADR 作为历史保留 Caddy 文字，当前部署文件和依赖仅使用 Nginx。

## 6. 开发启动问题修复复验（2026-10-09）

README 原先直接执行 createdb，但本机 5432 没有服务响应；新流程使用
`scripts/dev-db.sh` 初始化项目私有集群，实际在 127.0.0.1:55432 启动成功。
默认数据目录 `.dev-cache/postgres` 已验证被 Git 忽略。测试不修改系统 PostgreSQL。

在临时 Astro 配置中恢复旧 `ssr.noExternal: true`，实际复现输出：

```text
{"message":"Failed to create the dev server app: require is not defined","label":null,"level":"error"}
```

修复将完整依赖打包限定到 Astro build；开发模式由 Node 加载 CommonJS。
真实开发服务器 /about 实际返回 HTTP 200；开发代理跟随 SLORAY_API_INTERNAL_URLS
第一项，并新增 healthz/readyz 代理。复验命令：

```bash
python3 scripts/test-dev-db.py
cargo build --locked --bin sloray
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-dev.py
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-content.py
```

实际输出：

```text
PASS 开发数据库首次初始化、TCP 密码连接、私有权限、重复启动与重启保留数据
PASS Astro dev 实际页面、同源 API/登录/设置/探针代理、Markdown 清洗与内容即时更新
PASS 文章创建/发布/标签/修订，未登录和缺少 CSRF 拒绝写入，旧版本更新返回 409
PASS 会话与作者权限，公共详情支持 ETag/304
PASS 无 node_modules 的 SSR 发布包、内网 API 故障切换、Markdown 清洗、SEO、后台登录和 HTML ETag
PASS 撤回后旧 ETag 不会返回 304；软删除后后台和公开接口不可见
```

数据库测试还验证自定义端口在不再次指定变量时保持、密码不变、目录 0700 和密码 0600。
Astro dev 测试加入 CI Postgres 矩阵；开发数据库测试加入有服务端工具的恢复 job。

再次执行 fmt、clippy、53 项 Rust 测试、架构/依赖门禁、Astro check/build、
ShellCheck、actionlint 与 23 项发布切换/回滚检查，全部通过。
clippy 实际最后输出 `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.36s`；
Astro check 仍为 27 files / 0 errors / 0 warnings / 0 hints；
build 输出 `Server built in 1.31s` 和 `Complete!`，保留前述 5 条上游警告。
首次公告数据库更新遇到 TLS EOF，重试成功后实际输出仍为：

```text
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

新的 [开发指南](development.md) 与 [部署手册](deployment.md) 补充启动顺序、端口、
配置路径、.env 优先级、具体 PG 集群状态与常见错误。根 CLI 的 dotenvy 自动加载
当前目录/父目录 .env，已有进程环境变量优先；旧模板中的 5432 示例密码不能当作真实凭据。
新测试自动停止临时 API/SSR 和隔离集群；本次启动的项目开发集群也在验证后停止，数据保留。

## 7. 首版原型的许可证与提交前检查（2026-10-09）

根目录添加 MIT LICENSE；Cargo workspace 已声明 MIT，根 npm 包和 web 包同步声明 MIT。
发布组装脚本将 LICENSE 随 README 放入版本目录。统一使用 cargo-deny 0.20.2，
检查全部 workspace、全部功能、开发/构建依赖及传递依赖；私有 crate 不再跳过。

```bash
cargo deny --workspace --all-features --locked check licenses
cargo deny --workspace --all-features --locked list --format json --layout crate
./scripts/check-architecture.sh
```

实际输出：

```text
licenses ok
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

当前依赖图共 **399 个 crate/version**：9 个 workspace 成员和 390 个外部版本节点。
不是将依赖重新授权为 MIT，而是依据它们的 SPDX 表达式执行许可证白名单检查。
表达式求值与检查边界见 [cargo-deny 官方说明](https://embarkstudios.github.io/cargo-deny/checks/licenses/index.html)。

r-efi 5.3.0 与 6.0.0 声明 `MIT OR Apache-2.0 OR LGPL-2.1-or-later`，本版按 MIT 使用。
额外将当前 deny.toml 复制到临时文件，仅去除 LGPL allow 项，再以同一个 Cargo 依赖图运行：

```bash
cargo deny --workspace --all-features --locked --config /tmp/sloray-mit-release-deny.toml check licenses
```

同样实际输出 `licenses ok`，确认本版无需依赖 LGPL 分支满足许可策略。
项目原有 LGPL 政策仍保留；没有通过修改历史治理规则或添加豁免绕过检查。

首版提交前再次执行 fmt、clippy、cargo test（53 passed / 0 failed）、冻结前端锁文件安装、
api:check、Astro check/build、ShellCheck、actionlint 和发布切换/回滚检查，均成功。
check 输出 27 files / 0 errors / 0 warnings / 0 hints；build 成功，仍有 5 条已记录的上游警告。
源文件列表未包含运行时配置、setup token、数据库目录、依赖缓存或编译产物。

## 8. dev CI 安装与 PITR 竞态修复（2026-10-09）

针对 dev 提交 `3a260b4` 的 CI：actionlint 不在 install-action 支持列表中，
回退到 cargo-binstall 后找不到同名 Rust crate；改为下载官方 1.7.12 Linux amd64
发行包，按官方校验清单固定 SHA-256，再安装到 runner 的 PATH。
PITR 原脚本的 `00000000*` 不匹配 timeline 1 WAL，且数据库接受只读连接时
恢复可能尚未完成；改为核对指定 WAL 的大小与 `.done` 标记，并有界等待自动提升。
失败时保留命令 stderr、目标/WAL/恢复状态和两个集群日志，CI 上传诊断文件。

实际执行 CI 中的下载/校验/安装脚本与 actionlint，输出：

```text
actionlint.tar.gz: OK
1.7.12
installed by downloading from release page
built with go1.26.1 compiler for linux/amd64
PASS CI 官方 actionlint 下载、固定 SHA-256 校验、PATH 安装与流程检查
```

执行 `python3 scripts/test-pitr.py`，5 项回归覆盖只读阶段后的提升等待、等待超时、
错误 WAL/.backup/未完成或截断归档的拒绝、日志持久化及命令 stderr：

```text
Ran 5 tests in 0.002s
OK
```

PostgreSQL 18.6 隔离 PITR 连续执行三次，三次均实际输出：

```text
PASS 物理基础备份 + WAL 归档 + recovery_target_time 恢复并提升；保留目标前记录，排除目标后记录
```

另在真实隔离集群中注入 0.1 秒的归档等待超时，确认抛出 TimeoutError，
原始临时数据库清理后日志仍可读，且主库日志记录正常停止：

```text
PASS 真实 PG 归档超时注入：非零失败、日志保留、原始异常可见、隔离集群停止且数据清理
```

提交前执行 fmt、clippy（`--locked`）、workspace 测试（53 passed / 0 failed）、
`./scripts/check-architecture.sh`、前端 api:check/check/build、ShellCheck、actionlint，
以及发布切换/回滚、滚动发布、备份、发布包、Nginx 和开发数据库测试，均返回 0。
本地 Nginx 测试通过 `NGINX_BIN=/tmp/nginx-1.30.5/objs/nginx` 使用已准备的二进制。
实际门禁输出摘录：

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.32s
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
Result (27 files):
- 0 errors
- 0 warnings
- 0 hints
Server built in 1.23s
Complete!
PASS GET 在一个 API 返回 503 时切到健康上游
PASS 开发数据库首次初始化、TCP 密码连接、私有权限、重复启动与重启保留数据
```

Astro build 仍有此前记录的 5 条上游 MODULE_LEVEL_DIRECTIVE 警告，构建返回 0。
本次只修复 CI/演练脚本和对应说明，未递增版本号。

## 9. 原图上传失败诊断（2026-10-09）

本地开发上传显示“上传连接失败”。运行中的 API/Web 将输出写到终端，未保存可回溯
的服务日志文件；原图 PUT 从浏览器直达 R2，也不会经过 API 服务的请求日志。
只读检查实际配置的两个 bucket，并发送不创建对象的 CORS 预检，实际结果：

```text
ListObjectsV2(max_keys=0) 原图桶 HTTP 200
ListObjectsV2(max_keys=0) 派生桶 HTTP 200
OPTIONS Origin=http://localhost:4321 HTTP 403，无 Access-Control-Allow-* 响应头
OPTIONS Origin=http://127.0.0.1:4321 HTTP 403，无 Access-Control-Allow-* 响应头
```

仅请求 content-type，以及加入 if-none-match/校验和头的预检均被拒绝。
维护者确认原图桶尚未配置 CORS，选择稍后配置，先完成报错改进。
对象凭据可以读取两个桶，不等于有修改/读取桶配置的权限；GetBucketCors 返回 403。
这些只读检查未上传真实对象，也没有验证凭据的写入权限。

前端新增失败阶段、HTTP 状态、存储错误代码/请求编号（浏览器可读时）、上传编号、
本站 Origin、仅保留 origin 的存储端点和请求头名称。状态 0 提示可能是 CORS/网络/TLS，
不将隐藏的 HTTP 错误推断为已知状态。页面可展开详情，Console 记录相同的脱敏诊断；
不打印预签名路径/查询参数、头值或存储返回的 Message/响应原文。

Chromium 与隔离数据库/模拟存储实测拒绝 PUT（HTTP 403 / AccessDenied）、网络失败、
申请许可 API 403、诊断脱敏和失败后重新上传成功，实际输出：

```text
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 上传 HTTP/S3 错误与请求编号、网络/CORS 失败、API 失败阶段及诊断脱敏
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

提交前 fmt、clippy、workspace 测试（53 passed / 0 failed）、架构与依赖门禁、
前端 api:check/check/build 和上述真实浏览器检查通过，实际输出摘录：

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.65s
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
Result (28 files):
- 0 errors
- 0 warnings
- 0 hints
Server built in 1.32s
Complete!
```

build 仍有此前记录的 5 条上游 MODULE_LEVEL_DIRECTIVE 警告，返回 0。
本次未修改 R2 CORS、运行配置、数据库 schema 或版本号。

## 10. 开发 worker 启动与 PostgreSQL 18 CI（2026-10-09）

维护者成功上传原图后，页面持续显示处理中，公开桶没有派生图。检查运行进程仅有
`sloray serve`，没有 worker；只读数据库查询显示媒体未 verified、无宽高/派生时间，
media.generate 任务 attempts=0、last_error 为空，确认任务已入队但从未执行。
使用运行中 API 的同一份 `web/.dev-cache/runtime/config.json` 执行一次 `worker --once`，
返回 0。之后媒体 verified=true、width=884、height=1920、派生时间已写入；公开 CDN
的 640px WebP HEAD 返回 `HTTP/2 200`、`content-type: image/webp`。
维护者刷新媒体管理页后确认已恢复显示。未修改桶权限、CORS、凭据或原图。

新增 `scripts/dev-start.sh` 管理独立 API/Astro/worker，等待 API 就绪后启动 worker，
共用同一绝对配置路径；复用先前在 web 下创建的配置，两份配置同时存在时要求明确
选择。日志位于私有开发缓存目录，Ctrl+C/子进程失败清理本次进程组，保留数据库。
页面等待/超时提示明确原图已经上传，并提醒检查 worker。开发文档将 worker 从
“可选”改为媒体、定时发布和 outbox 的必要进程，说明存储配置修改后的重启要求。

CI 迁移/集成作业只使用 PostgreSQL 18 与 18 客户端，移除 16/17 的重复作业；
全部原有集成测试和隔离 PITR 保留，并加入开发启动器的进程回归与真实媒体联动测试。
运行时 PostgreSQL ≥16 的兼容要求保持不变。

执行 `python3 scripts/test-dev-start.py`，临时目录中的真实 HTTP/进程组实测输出：

```text
PASS 旧 web 配置复用、API 就绪后启动 worker、Ctrl+C 清理进程及孙进程、日志私有权限
PASS 相对路径共用配置、worker 失败停止全部服务、未输出配置密钥
PASS 端口冲突及两份配置时明确拒绝启动，不接管既有进程
```

在 PostgreSQL 18.6 上使用隔离测试数据库、本地模拟对象存储执行
`python3 scripts/smoke-dev-start.py`，实际调用统一启动脚本：

```text
PASS 统一启动真实 API/Astro/worker、同源代理、自动消费媒体任务及公开 WebP 可访问
PASS 统一启动器 Ctrl+C 停止真实服务，API / Astro 端口均释放
```

执行 `python3 scripts/smoke-browser.py`，Chromium 上传与封面回归输出：

```text
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 上传 HTTP/S3 错误与请求编号、网络/CORS 失败、API 失败阶段及诊断脱敏
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

提交前执行 fmt、clippy、workspace 测试（53 passed / 0 failed）、架构/依赖门禁、
前端 api:check/check/build、ShellCheck、actionlint、发布切换/回滚及 `git diff --check`，
均返回 0。输出摘录：

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
Result (28 files):
- 0 errors
- 0 warnings
- 0 hints
Server built in 877ms
Complete!
```

build 仍有已记录的 5 条上游 MODULE_LEVEL_DIRECTIVE 警告，返回 0。
本次未递增版本号，也未推进 main。

## 11. 后台页面管理、Markdown 预览与媒体选择器（2026-10-09）

普通页面改为 Postgres `pages` 内容，新增第五条 expand 迁移，默认 about 迁入数据库。
管理员可创建/编辑/发布/撤回/改址/删除页面，设置导航显示和排序，sitemap 包含已发布页面。
访客的两处写作入口均移除；直接访问 /login 登录后显示管理后台，后台按角色统一导航。
文章与页面共用编辑器和服务端 Markdown 清洗；支持分栏预览、明确保存状态、快捷保存、
未保存离开保护及通过媒体选择器上传/复用/复制 Markdown。封面从图片库选择和移除。

执行 fmt、clippy（`--locked`）、workspace 测试、架构/依赖门禁、前端
api:check/check/build、OpenAPI 引用/兼容性与生成漂移、actionlint、发布切换/回滚、
`git diff --check`。workspace 共 54 项测试通过，实际门禁输出摘录：

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.87s
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
PASS OpenAPI 本地引用与唯一 operationId
PASS OpenAPI 向后兼容性检查
Result (38 files):
- 0 errors
- 0 warnings
- 0 hints
Server built in 1.13s
Complete!
```

在仅监听回环的临时 PostgreSQL 18.6 集群运行独立数据库测试：
`smoke-pages.py`、`smoke-migrations.py`、`smoke-content.py`、`smoke-backup.py`，
每项创建/删除隔离数据库。实际输出：

```text
PASS 页面管理员权限/CSRF、保留地址、版本冲突、游标、发布/改址/撤回/删除、导航和 sitemap
PASS 页面 SSR 与 Markdown 预览一致、脚本/危险链接清洗、预览认证与长度限制、purge outbox
PASS 五条迁移 Applied；down all → up、down 1 → up、fresh，18 张表重建成功
PASS 文章创建/发布/标签/修订，未登录和缺少 CSRF 拒绝写入，旧版本更新返回 409
PASS 会话与作者权限，公共详情支持 ETag/304
PASS 无 node_modules 的 SSR 发布包、内网 API 故障切换、Markdown 清洗、SEO、后台登录和 HTML ETag
PASS 撤回后旧 ETag 不会返回 304；软删除后后台和公开接口不可见
PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，18 张表内容与迁移记录逐表一致
```

执行 `smoke-browser.py`，Chromium 与真实 API/独立 SSR、本地模拟对象存储和 worker
完成端到端操作，移动宽度 390px 无横向溢出；公开页面、编辑器、媒体对话框与页面管理
执行 axe，未发现所测 WCAG 标签范围的违规。浏览器返回与站内链接离开时取消提示会
保留正文；定时发布要求日期，切到立即发布会清除未来日期。实际输出：

```text
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 上传 HTTP/S3 错误与请求编号、网络/CORS 失败、API 失败阶段及诊断脱敏
PASS Markdown 实时预览/分栏/清洗、未保存离开提示、移动布局、媒体复用与 Markdown 剪贴板及拒绝权限退路
PASS 后台创建页面、草稿/发布/改址/导航排序/撤回/删除、页面 axe 与退出后隐藏管理入口
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

build 仍有既有的 5 条上游 MODULE_LEVEL_DIRECTIVE 警告，返回 0。
临时集群在验证后停止；没有对现有开发/线上数据库执行迁移或使用真实对象存储。
现有部署启用该功能需要先按正常发布流程执行 `sloray migrate up`；未递增版本号或发布。

## 12. Material 3、Astro 群岛与主题包（2026-10-09）

新增 JSON＋CSS 主题库与发布快照；第六条 expand 迁移只增加 themes / theme_settings。
管理员可导入、导出、隔离预览、保存、启用、恢复和删除主题。启用同时核对主题库版本和
启用记录版本。公开站点采用可配置 Material 配色、字体、宽度、导航、列表和首页区块；
后台、login、setup 复用内置 Material 样式与本地 SVG 图标。Astro 组件复用结构，原生
自定义元素维护独立交互，server:defer 延迟读取主题工作台和可选页面目录。

以下命令实际执行：

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.33s
cargo test --locked --workspace
    59 passed; 0 failed（汇总各 crate 的 test result）

target/debug/sloray openapi | cmp - docs/api/openapi.json
python3 scripts/check-api.py --validate
PASS OpenAPI 本地引用与唯一 operationId
python3 scripts/check-api.py --base-ref HEAD
PASS OpenAPI 向后兼容性检查
python3 scripts/test-api-contract.py
Ran 6 tests ... OK
pnpm --dir web run api:check
✨ openapi-typescript 7.13.0
pnpm --dir web run check
Result (51 files): 0 errors / 0 warnings / 0 hints
pnpm --dir web run test
tests 8 / pass 8 / fail 0
pnpm --dir web run build
[build] Complete!
```

构建仍有五个既有上游 MDX / hast-util-to-estree MODULE_LEVEL_DIRECTIVE 警告，构建退出 0。
未添加任何 Rust/npm 依赖；Material SVG 路径与改编 symbol 附带 Apache-2.0 许可证及来源。

本轮 `./scripts/check-architecture.sh` 曾完整通过，实际输出为：

```text
== 单文件行数上限：500 行（*.rs 与 *.sh） ==
  ✓ 全部合规
advisories ok, bans ok, licenses ok, sources ok
  ✓ advisories / bans / licenses / sources 全部通过
  ✓ crate 依赖方向 / workspace 版本引用 / unsafe 禁令通过
结果：全部通过
```

最终重跑的行数和 crate 门禁通过，但 GitHub RustSec 在线更新连续遇到 TLS connect error /
unexpected eof while reading，脚本退出 1。没有改动依赖图或放宽 deny 策略；使用本轮已更新的
公告库再次执行四项完整检查（缓存提交 550efd3，2026-10-08T16:47:14+02:00）：

```text
cargo deny --offline --workspace --all-features --locked --log-level error check advisories bans licenses sources
advisories ok, bans ok, licenses ok, sources ok
```

网络失败不能视为最新在线公告检查成功；push CI 仍执行原样的在线门禁。

在 /tmp 独立 PostgreSQL 集群的 UUID 临时数据库上，实际运行 smoke-setup / dev / dev-start /
content / pages / themes / activity / search / media / migrations / backup，全部退出 0。
主题和备份在最终版本检查改动后再次执行，实际输出：

```text
PASS 主题包校验、鉴权/CSRF、并发启用冲突、发布快照、SSR 布局、样式注入隔离、群岛鉴权、恢复与删除
PASS 六条迁移 Applied；down all → up、down 1 → up、fresh，20 张表重建成功
PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，20 张表内容与迁移记录逐表一致
```

测试包括作者不能管理主题、过期库版本不能启用、保存已启用主题不会改动公开快照、启用使文章
旧 HTML ETag 返回 200，以及 Rust / Astro 默认主题契约一致。备份实际包含一个启用中的主题。
迁移、备份与 SSR 使用独立临时数据库和无 node_modules 的 dist 发布目录，不访问现有部署数据。

最终构建的真实 Chromium 浏览器回归输出：

```text
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 上传 HTTP/S3 错误与请求编号、网络/CORS 失败、API 失败阶段及诊断脱敏
PASS Markdown 实时预览/分栏/清洗、未保存离开提示、移动布局、媒体复用与 Markdown 剪贴板及拒绝权限退路
PASS 主题群岛加载、CSS/布局预览、导入导出、启用快照、恢复删除、外观持久化及 360/768/1440px axe/布局审计
PASS 后台创建页面、草稿/发布/改址/导航排序/撤回/删除、页面 axe 与退出后隐藏管理入口
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

截图审阅发现主题变量曾被基础 CSS 覆盖，修复为 cascade layer 并加入真实颜色/列数断言。
桌面、移动端与深色界面经截图审阅；移动端 iframe 预览滚动到可见区域后检查正文实际渲染。

另外实际执行 deploy/scripts/test-activate-release.sh，输出“结果：全部通过”。
使用此前编译的 /tmp/nginx-1.30.5/objs/nginx 运行 scripts/test-nginx.py，输出：

```text
nginx: configuration file ... test is successful
PASS POST 失败仅投递一次，未重放到另一 API
PASS Nginx 配置、API/安装/登录/设置/SSR 分流、真实 IP 头覆盖
PASS GET 在一个 API 返回 503 时切到健康上游
PASS 全部上游不可用时正确返回失败
```

新增 /admin/themes 与 /_server-islands/ThemeWorkbench 路径分流断言。
actionlint 1.7.12、git diff --check 退出 0。新前端单元回归、主题 API 冒烟、主题浏览器流程及
20 表备份核对已接入 CI；尚未推送，以上结果不等于远程 CI 成功。

## 13. 开发启动器适配页面与主题（2026-10-09）

`dev-start.sh` 每次按冻结锁文件同步前端依赖，检查 Node ≥22.12；API 数据库就绪后，
使用与 API / worker 相同的绝对配置路径执行 `migrate up`。迁移成功后启动 worker，
迁移失败则停止本次 API / Astro；首次未配置或数据库不可达时保留安装入口。
迁移进程也纳入 Ctrl+C / SIGTERM 进程组清理，日志为私有 `migrate.log`。

实际执行 `python3 -u scripts/test-dev-start.py`，输出：

```text
PASS 首次依赖安装、旧 web 配置复用、等待数据库和迁移后启动 worker、私有日志及 Ctrl+C 清理
PASS 相对路径共用配置、worker 失败停止全部服务、未输出配置密钥
PASS 迁移失败阻止 worker，停止 API/Astro 并提示迁移日志
PASS Ctrl+C 中断迁移时清理迁移进程组及子进程
PASS 锁定依赖安装失败时不启动任何业务服务
PASS 无配置首次启动保留 /setup，SIGTERM 停止全部服务
PASS 端口冲突及两份配置时明确拒绝启动，不接管既有进程
```

在 `/tmp` 独立 PostgreSQL 集群的 UUID 临时库上执行 `scripts/smoke-dev-start.py`。
只将该一次性库降至四条迁移，保留既有文章，再调用真实启动器验证升级。
重复启动等待迁移完成和 worker 启动后，核对文章、默认页面与已启用主题未被覆盖。
实际输出：

```text
PASS 自动升级四迁移旧库到六迁移/20 表、保留原有文章、页面与主题 API/SSR/群岛可用
PASS 统一启动真实 API/Astro/worker、同源代理、自动消费媒体任务及公开 WebP 可访问
PASS 统一启动器 Ctrl+C 停止真实服务，API / Astro 端口均释放
PASS 已升级开发库重复启动幂等，不重建页面、不覆盖已启用主题或文章
```

其余实际门禁：`cargo fmt --all -- --check`、
`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace` 均退出 0；Rust 各测试组实际为
9 / 10 / 7 / 4 / 11 / 7 / 11 项通过，合计 59 项。
`pnpm --dir web run api:check`、`test`、`check`、`build` 均退出 0，实际输出摘录：

```text
ℹ tests 8
ℹ pass 8
ℹ fail 0
Result (51 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
```

构建仍有前节记录的五条上游 MDX/HAST `MODULE_LEVEL_DIRECTIVE` 提示。
本次 `./scripts/check-architecture.sh` **在线执行成功**，实际输出：

```text
== 单文件行数上限：500 行（*.rs 与 *.sh） ==
  ✓ 全部合规
advisories ok, bans ok, licenses ok, sources ok
  ✓ advisories / bans / licenses / sources 全部通过
  ✓ crate 依赖方向 / workspace 版本引用 / unsafe 禁令通过
结果：全部通过
```

`deploy/scripts/test-activate-release.sh` 实际输出“结果：全部通过”；
`bash -n`、Python 编译检查与 `git diff --check` 退出 0。
测试完成后删除临时测试库并停止专用测试集群；未访问现有开发或生产数据库。
以上是本地验证，未推送，不能替代远程 CI。


## 响应式 Dock、首页编排与站点标识（2026-10-10）

本次新增第七条 expand 迁移，借用项目已有 PostgreSQL 18.6 服务，在 UUID 临时库上测试；
没有对开发库执行迁移、清空或写入测试文章。启动阶段沙箱进程检查误判导致开发数据库服务停止，
随后已恢复原服务，复用原数据目录。测试创建的数据库和 API/SSR/worker 进程均已清理。

实际执行 `cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace`，全部退出 0。Rust 各非空测试组为 9/10/9/4/11/7/11，合计 61 项。
`pnpm --dir web run api:check`、`test`、`check`、`build` 全部退出 0。实际输出摘录：

```text
ℹ tests 9
ℹ pass 9
ℹ fail 0
Result (61 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
```

构建仍有五条既有上游 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示。
`./scripts/check-architecture.sh` 在线执行成功，实际输出：

```text
advisories ok, bans ok, licenses ok, sources ok
  ✓ advisories / bans / licenses / sources 全部通过
  ✓ crate 依赖方向 / workspace 版本引用 / unsafe 禁令通过
结果：全部通过
```

`python3 scripts/check-api.py --validate` 和 `--base-ref HEAD`（相对本次开发前提交）均通过。
部署切换与回滚脚本实际输出“结果：全部通过”。
执行 smoke-migrations.py、smoke-backup.py、smoke-pages.py、smoke-themes.py、smoke-experience.py 和 smoke-dev-start.py，实际输出摘录：

```text
PASS 七条迁移 Applied；down all → up、down 1 → up、fresh，21 张表重建成功
PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，21 张表内容与迁移记录逐表一致
PASS 站点名称/Favicon 持久化、鉴权/CSRF、并发冲突、outbox、首页编排与 Astro 组件重载/文本转义
PASS 自动升级四迁移旧库到七迁移/21 表、保留原有文章、页面与主题 API/SSR/群岛可用
PASS 已升级开发库重复启动幂等，不重建页面、不覆盖已启用主题或文章
```

真实 Chromium `scripts/smoke-browser.py` 的实际输出：

```text
PASS Dock 展开/Escape、360/768/1440 布局/axe、减少动态效果、群岛独立刷新、字号持久化、专注阅读与目录
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 设置页站点名称/Favicon 保存、页面标题/导航/RSS 同步与恢复默认
PASS 上传 HTTP/S3 错误与请求编号、网络/CORS 失败、API 失败阶段及诊断脱敏
PASS Markdown 实时预览/分栏/清洗、未保存离开提示、移动布局、媒体复用与 Markdown 剪贴板及拒绝权限退路
PASS 首页编排添加组件域、便笺/页面组件、保存启用、SSR 群岛加载及独立刷新
PASS 主题群岛加载、CSS/布局预览、导入导出、启用快照、恢复删除、外观持久化及 360/768/1440px axe/布局审计
PASS 后台创建页面、草稿/发布/改址/导航排序/撤回/删除、页面 axe 与退出后隐藏管理入口
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

审计覆盖 360/768/1440px、深浅模式、手机折叠及展开状态和减少动态效果。
截图使用隔离测试站点内容。以上为本地验收，未执行远程 push CI 或生产发布。


## 纯图标 Dock、随动目录与背景设置（2026-10-10）

使用已有 PostgreSQL 服务创建 UUID 临时库；没有对现有开发库执行迁移或写入测试内容。
本次不增加数据库迁移或依赖。测试进程与临时库均由 smoke_support 清理。

实际执行 fmt、clippy（workspace/all-targets，-D warnings）、workspace test、架构门禁、
前端 api:check/test/check/build、OpenAPI validate/向后兼容检查、git diff --check，全部退出 0。
Rust 非空测试组为 9/10/10/4/11/7/11，合计 62 项。实际输出摘录：

```text
ℹ tests 11
ℹ pass 11
ℹ fail 0
Result (63 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
PASS OpenAPI 本地引用与唯一 operationId
PASS OpenAPI 向后兼容性检查
```

构建仍有五条既有 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示。
部署切换/回滚脚本退出 0，实际输出“结果：全部通过”。
smoke-themes.py 使用最新 Rust 二进制验证背景 URL/数值边界、鉴权/CSRF、并发版本、
保存与启用快照、SSR/CSS、ETag 和恢复删除，退出 0。

真实 Chromium smoke-browser.py 最终完整执行退出 0，实际输出摘录：

```text
PASS 文章/页面 SSR 目录、桌面随动/章节高亮、手机展开/锚点焦点/Escape、动态标题重载、3840px 宽度与无 JS 目录
PASS 纯图标 Dock 展开/Escape、360/768/1440/3840 布局/axe、减少动态效果、群岛独立刷新、字号持久化与专注阅读
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 背景/遮罩/背景模糊/面板模糊预览与保存启用、导出导入、后台隔离及恢复纯色
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

浏览器回归也通过站点标识保存、Markdown/媒体/剪贴板、首页编排与主题导入导出、
页面创建/发布/撤回/删除；深浅色、减少动态效果和宽主题 Dock 间距均覆盖。
验证中修复了锚点小数像素高亮边界、Astro 自定义元素 hidden=false 的 SSR 隐藏问题，
并为可横向滚动的代码块增加键盘焦点。以上为本地验证，未执行远程 push CI 或生产发布。

## 页面上下文 Dock 与动作弹窗（2026-10-10）

本次按 ADR-0023 合并 Favicon 主页入口、内置页面图标、文章/后台编辑操作集、
搜索与评论 dialog、摘要和持久切页。第八条 expand 迁移只增加 pages.dock_icon，
旧页面及省略字段的旧客户端使用 article。测试使用现有 Postgres 的 UUID 临时库，
没有给开发库写入测试内容或应用迁移。

实际执行并退出 0：
`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace`（63 项，0 失败）、`./scripts/check-architecture.sh`、
`pnpm run api:check`、`test`、`check`、`build`（web 目录）、`git diff --check`。
OpenAPI 本地引用、与 HEAD 契约兼容检查及 6 项契约检查器测试均通过。实际输出：

```text
ℹ tests 11
ℹ pass 11
ℹ fail 0
Result (67 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

构建仍显示既有上游 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示；没有新增依赖。
`deploy/scripts/test-activate-release.sh` 退出 0，输出“结果：全部通过”。
在先构建当前 Rust 二进制与 Astro SSR 后，实际执行（统一设置 SLORAY_TEST_DATABASE_URL）：

```text
python3 scripts/smoke-migrations.py
PASS 八条迁移 Applied；down all → up、down 1 → up、fresh，21 张表重建成功
python3 scripts/smoke-pages.py
PASS 页面管理员权限/CSRF、保留地址、版本冲突、游标、发布/改址/撤回/删除、导航和 sitemap
PASS 页面 SSR 与 Markdown 预览一致、脚本/危险链接清洗、预览认证与长度限制、purge outbox、Dock 图标默认/持久化/白名单校验
python3 scripts/smoke-backup.py
PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，21 张表内容与迁移记录逐表一致
python3 scripts/smoke-dev-start.py
PASS 自动升级四迁移旧库到八迁移/21 表、保留原有文章、页面与主题 API/SSR/群岛可用
PASS 已升级开发库重复启动幂等，不重建页面、不覆盖已启用主题或文章
python3 scripts/smoke-browser.py
PASS Favicon 主页、按页面切换操作、评论取消/放弃草稿、黑色搜索弹窗、320px Dock、AJAX 节点与动画时间线连续
PASS 禁用 moveBefore 的旧浏览器路径保持 Dock 节点、CSS 动画时间及弹窗监听
PASS 浏览器局部导航、前进后退、评论提交、中文搜索、SEO 与公开页 axe 检查
PASS 后台创建页面、草稿/发布/改址/导航排序/撤回/删除、页面 axe 与退出后隐藏管理入口
```

浏览器同时通过深浅主题、减少动态效果、320/360/768/1440/3840px 布局、随动目录、
群岛刷新、Markdown/媒体/剪贴板、后台连续保存、点赞/审核及主题与背景配置。
自定义页面选择 palette 后，公开 Dock 显示对应图标。评论取消历史返回时，
URL、弹窗与输入均保留；确认放弃后可以正常前进/后退。搜索打开 400ms 后仍停留原文章。
对新旧动画移动路径都检查同一 Dock/主页节点，并校验原动画已播放时间没有归零。
人工查看文章、搜索与评论截图，摘要、黑色遮罩和 Dock 布局符合当前要求。
部署已有环境须先应用第八条迁移或重新启动统一开发脚本完成自动迁移；本地通过不等于远程 CI。

## MD3 切页、Dock 新动作与图像聚焦（2026-10-10）

本次按 ADR-0024 移除正文缩放，采用 MD3 曲线的滑出淡出/滑入淡入；新增 Dock 操作
仅在首次可见时缓动入场。Markdown 图片宽高作为正文属性保存，媒体库提供尺寸，
编辑器在保存前读取外部图片缺失尺寸；文章和页面使用同一个清洗后加载控件。
本次没有新增依赖或数据库迁移；真实数据库测试使用 UUID 临时库，不给开发库写入测试内容。

实际执行并退出 0：`cargo fmt --all -- --check`、
`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace`（63 项，0 失败）、`./scripts/check-architecture.sh`、
web 目录的 `pnpm run api:check`、`test`、`check`、`build`，以及 `git diff --check`。
实际输出摘录：

```text
ℹ tests 13
ℹ pass 13
ℹ fail 0
Result (72 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

构建仍显示既有上游 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示。
先构建当前 Rust 二进制和 Astro SSR，再设置 SLORAY_TEST_DATABASE_URL 实际执行：

```text
python3 scripts/smoke-pages.py
PASS 页面管理员权限/CSRF、保留地址、版本冲突、游标、发布/改址/撤回/删除、导航和 sitemap
PASS 页面 SSR 与 Markdown 预览一致、脚本/危险链接清洗、预览认证与长度限制、purge outbox、Dock 图标默认/持久化/白名单校验
python3 scripts/smoke-browser.py
PASS Favicon 主页、按页面切换操作、评论取消/放弃草稿、黑色搜索弹窗、320px Dock、AJAX 节点与动画时间线连续
PASS 禁用 moveBefore 的旧浏览器路径保持 Dock 节点、CSS 动画时间及弹窗监听
PASS MD3 前进/返回滑移淡入淡出、无缩放、Dock 新按钮缓动、图片原比例占位/聚焦/零漂移/失败、减少动态效果与无 JS
PASS Markdown 实时预览/分栏/清洗、未保存离开提示、移动布局、媒体复用与 Markdown 剪贴板及拒绝权限退路
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
```

完整浏览器回归还通过随动目录、320/360/768/1440/3840px 布局、深浅主题、主题与背景配置、
首页编排/群岛刷新、搜索/评论弹窗、历史导航、自定义页面图标及无 JS 阅读。
新增断言检查真实 CSS 关键帧的退出/进入曲线与方向，原有 Dock 操作不重新入场。
延迟图片响应时横图按 2:1 原始比例占位；加载完成前后容器高度及下方正文位置误差小于 0.1px。
人工查看棋盘纹理的占位、聚焦中间帧和最终截图，模糊消失、色块淡出，最终透明度为 1。
失败的竖图保留 1:2 比例，减少动态效果关闭模糊/渐变，无 JS 图片仍可见。
媒体库复制携带 400×200 原图尺寸；手工去除属性后的外部链接在保存时恢复该派生图的
真实解码宽高，并通过 API 验证正文和修订历史都保留属性。没有执行远程 push CI 或发布。

## 后台持久 Tab、Zen 过渡与统一 Toast（2026-10-10）

按 ADR-0025 将后台 Tab 放入 main 外的持久 chrome，取消 page-content 快照的默认
尺寸插值，保持自动高度、contain 和顶部对齐。管理 Dock 使用服务器图标；独立页面
不渲染阅读进度，文章进度轨道透明。Astro 与 Rust 登录/设置页共享彩色 Toast，弹窗
开启时通知进入弹窗的 DOM 子树并在顶层显示，保证关闭按钮可点击且不夺取输入焦点。
本次没有新增依赖、数据库字段或迁移。

在上述 MD3 验证后追加并实际执行，命令均退出 0：
`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace`（63 项，0 失败）、`./scripts/check-architecture.sh`，
以及 web 的 `pnpm run api:check`、`test`、`check`、`build` 与 `git diff --check`。
实际输出摘录：

```text
ℹ tests 13
ℹ pass 13
ℹ fail 0
Result (76 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

重新构建 Rust 二进制和 Astro SSR 后，使用 UUID 隔离数据库执行
`python3 scripts/smoke-pages.py` 与 `SLORAY_TEST_ZEN=1 python3 scripts/smoke-browser.py`，
每次只向临时库写入测试内容并自动清理。数据库连接通过 SLORAY_TEST_DATABASE_URL 提供。
完整 Chromium/axe、内容/API 与原生 Zen 输出包括：

```text
PASS 页面 SSR 与 Markdown 预览一致、脚本/危险链接清洗、预览认证与长度限制、purge outbox、Dock 图标默认/持久化/白名单校验
PASS 后台 Tab 持久且位于动效外、快照无尺寸插值、服务器图标、页面无进度/文章透明轨道、Toast 颜色/顶层、不可达图片不阻断保存
PASS 设置页站点名称/Favicon 保存、页面标题/导航/RSS 同步与恢复默认
PASS 主题群岛加载、CSS/布局预览、导入导出、启用快照、恢复删除、外观持久化及 360/768/1440px axe/布局审计
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
PASS Zen 1.23.1b 媒体/主题切页快照无拉伸、后台 Tab 保持节点、Toast 顶层显示/弹窗内可点击
```

媒体保存回归主动阻断派生图片的网络请求，缺失属性仍从媒体元数据补齐为原图
400×200，并经 API 检查正文及修订均保留尺寸；封面保存不依赖新的图片下载。
另创建不可达的正文图片，验证保存成功、正文不丢失、完整导航后显示黄色警告 Toast。
这取代上一节“重新测量派生图尺寸”的保存策略。文章及图片 MD3 动效、零布局漂移、
无 JS 阅读、手机 Dock/目录、历史与评论草稿保护等回归继续通过。

Zen 使用本机安装的 1.23.1b、原生 WebDriver BiDi 和独立临时 profile，截取媒体/主题
进入动画中的帧，人工确认字体、表单和预览比例正常。截图位于
`/tmp/sloray-zen-media-transition.png` 与 `/tmp/sloray-zen-themes-transition.png`。
可用 `SLORAY_TEST_ZEN=only` 单独执行 Zen 回归；该脚本仅关闭自己启动的测试浏览器。
构建仍有既有上游 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示，未执行远程 push CI 或发布。

## 切页叠画与 Zen 压缩回归修复（2026-10-10）

上一节的 Zen 验证不足：没有断言截图时动画确实暂停在中间位置，BiDi 截图也不能证明
浏览器快照层的实际绘制。先前“切页快照无拉伸”的结论不能覆盖维护者报告的故障。
重查原生动画发现旧、新快照同时可见；未指定 both 填充，新内容在延迟期提前显示，
旧内容退出后又恢复可见。延迟加载的主题群岛还使快照高度与实际正文不一致。

按 ADR-0026 改用真实 main 的串行动画：加载成功后 200ms 滑出淡出，替换后 350ms
滑入淡入，保持 MD3 曲线、前进/返回方向与无缩放。跳过原生快照层，Dock 与后台 Tab
继续持久化。退出和进入分别保持端点状态，中止导航恢复可见正文。

新的 Chromium 回归冻结真实 Web Animations API 动画，断言时间、playState、MD3 曲线、
交接透明度及旧正文断开。Zen 1.23.1b 使用独立临时 profile，将退出和进入分别暂停于
120ms，并从 PNG 像素验证 240×64 标记保持原尺寸、未出现另一页的标记；标记的实际
颜色也必须处于淡出或淡入状态，防止把最终页面误当中间帧。
覆盖首页到文章、长文章返回首页、主题到账号、账号到主题、主题到媒体。
主题群岛加载后仍保持预览高度；Dock 和后台 Tab 保持同一节点。
人工查看主题退出及进入的原生截图，字体、表单和预览比例正常。

跳过快照会按规范拒绝 ready Promise；在跳过前注册拒绝处理，避免未处理的
“Transition was skipped”。实际 updateCallbackDone 仍由 Astro 观察，未隐藏换页错误。
连续导航回归另覆盖没有原生 View Transitions 的兼容路径、退出/进入期间取消、
减少动态效果及网络失败转完整导航前恢复正文。保留原生 API 的独立验证也通过同样的
取消与失败恢复断言，pageerror 为 0，没有用测试拦截器处理原生 ready Promise。

实际执行并退出 0：`cargo fmt --all -- --check`、
`cargo clippy --locked --workspace --all-targets -- -D warnings`、
`cargo test --locked --workspace`（各 crate 合计 63 项，0 失败）、
`./scripts/check-architecture.sh`、web 的 `pnpm run api:check`、`test`、`check`、`build`，
以及 `git diff --check`。本次没有 Rust、依赖或数据库迁移变更。
实际输出摘录：

```text
ℹ tests 13
ℹ pass 13
ℹ fail 0
Result (79 files):
- 0 errors
- 0 warnings
- 0 hints
[build] Complete!
advisories ok, bans ok, licenses ok, sources ok
结果：全部通过
```

在最终构建上使用 UUID 隔离数据库执行
`SLORAY_TEST_ZEN=1 python3 scripts/smoke-browser.py`，退出 0。
完整 Chromium/axe 与原生 Zen 回归实际输出包括：

```text
PASS 正文真实中间帧、串行退出/进入/交接无叠画、跳过快照、MD3 前进/返回无缩放、Dock 入场、图片聚焦/零漂移/减少动态效果/无 JS
PASS 无原生 View Transitions 的 AJAX 单一正文、连续导航取消退出/进入、Dock 节点/时间线、减少动态效果与失败完整导航前恢复正文
PASS 后台 Tab 持久且位于动效外、跳过快照、服务器图标、页面无进度/文章透明轨道、Toast 颜色/顶层、不可达图片不阻断保存
PASS 主题群岛加载、CSS/布局预览、导入导出、启用快照、恢复删除、外观持久化及 360/768/1440px axe/布局审计
PASS HTML 登录、后台连续保存、跨域直传与封面展示、点赞、审核、删除媒体及管理页 axe 检查
PASS Zen serial home-post: departing 240×64, arriving 240×64, no stale pixels
PASS Zen serial long-post-home: departing 240×64, arriving 240×64, no stale pixels
PASS Zen serial themes-users: departing 240×64, arriving 240×64, no stale pixels
PASS Zen serial users-themes: departing 240×64, arriving 240×64, no stale pixels
PASS Zen serial themes-media: departing 240×64, arriving 240×64, no stale pixels
PASS Zen 1.23.1b 公众/后台切页真实中间帧无重叠/压缩、长短正文原尺寸、后台 Tab/Dock 保持节点、Toast 顶层显示/弹窗内可点击
```

Zen 截图为 `/tmp/sloray-zen-{home-post,long-post-home,themes-users,users-themes,themes-media}-{departing,arriving}.png`；
标记和长正文填充仅由测试注入。构建仍有既有上游 MDX/HAST MODULE_LEVEL_DIRECTIVE 提示。
未执行远程 push CI 或发布。
