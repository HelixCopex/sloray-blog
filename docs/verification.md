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
