# sloray-blog

Rust（axum + SeaORM）+ Astro SSR 的动态博客，单台 VPS 原生部署，目标反向代理是 Nginx。

启动项目先读 [开发指南](docs/development.md)；改代码前读 [架构](docs/architecture.md) 和
[贡献指南](AGENTS.md)，部署主机时读 [部署手册](docs/deployment.md)。
专项说明：[中文检索](docs/search.md)、[媒体](docs/media.md)、[备份恢复](docs/backup.md)、
[ADR](docs/adr/README.md)、[本地验证记录](docs/verification.md)。

## 功能

文章/标签、Markdown 写作、草稿/发布/定时发布、修订与并发编辑保护；
数据库登录会话、作者权限和管理员账号创建；评论先审后发、回复、蜜罐与限流；
中文全文搜索、去重浏览、幂等点赞；私有图片直传、公开 WebP 派生、封面与媒体删除。
提供 RSS、sitemap、JSON-LD、运行时 canonical、局部导航与 HTTP ETag 重验证。

Postgres 保存权威业务记录，外部对象操作经 outbox。原图与公开图使用不同 bucket，
媒体/会话不落 API 本地磁盘；配置 shared/ 是唯一部署期可写目录。

## 本地开发

依赖 Rust ≥ **1.94.1**、Node ≥ **22.12**、pnpm **12.9.1**、Postgres ≥ **16**。
图像与 SDK 的本地构建需要 C 工具链、pkg-config 和 CMake。中文 PG 扩展可选，
没有扩展时自动应用分词。Nginx 本地可选，Astro 开发服务器代理 API/登录/设置。

以下终端均从仓库根目录开始。首次安装数据库与工具链的方法见 [开发指南](docs/development.md)。

**1. 准备数据库**（普通用户运行，无需 sudo）：

```bash
./scripts/dev-db.sh start
./scripts/dev-db.sh status
./scripts/dev-db.sh url
```

脚本自动初始化项目专用 PostgreSQL，监听 `127.0.0.1:55432`，创建 `sloray` 库。
数据与随机密码保存在被 Git 忽略的 `.dev-cache/postgres/`，重启保留，不改系统数据库。
保存最后输出的连接串，稍后填入向导。`createdb` 只创建库，本身不会启动数据库服务。

**2. 终端 A：启动 API**（首次编译可能较久）：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- serve
```

**3. 终端 B：获取 token，再启动前端**：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- setup-token
cd web
pnpm install --frozen-lockfile
pnpm run dev
```

打开 **http://127.0.0.1:4321/setup**，填写 token、步骤 1 的连接串、管理员，以及站点地址
`http://127.0.0.1:4321`。向导自动建表；随后访问首页和 `/admin/posts`。开发代理连接 API 8080，
API 与 SSR 都保持运行。已有配置时直接打开首页和 `/login`。

未配置时 healthz=200、readyz=503，setup 可访问。数据库暂不可达可通过 token
恢复；损坏 JSON 需先修复文件。生产必须使用应用账户独占的共享目录。
本地使用媒体或定时发布时，另开终端从根目录运行：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- worker
```

结束时在前端/API/worker 终端按 Ctrl+C，再运行 `./scripts/dev-db.sh stop`，数据保留。
若 Astro 以后台方式启动，在 web 运行 `pnpm exec astro dev stop`。
端口冲突、系统数据库替代方案、环境变量与两条常见错误见 [开发排错](docs/development.md#6-常见错误)。
生产安装的 systemd timer 自动执行 worker --once。

## 常用命令

```bash
cargo run --bin sloray -- serve
cargo run --bin sloray -- setup-token --rotate
cargo run --bin sloray -- migrate up
cargo run --bin sloray -- migrate status
cargo run --bin sloray -- migrate down 1     # 仅本地验证，不用于生产回滚
cargo run --bin sloray -- worker --once
cargo run --bin sloray -- rebuild-search
cargo run --bin sloray -- rebuild-stats
cargo run --bin sloray -- rebuild-media
cargo run --bin sloray -- media-gc           # 默认 dry-run
cargo run --bin sloray -- openapi
cargo run --bin sloray -- systemd print
```

CLI 用 SLORAY_CONFIG_FILE 或 --config 选择配置；环境变量覆盖 JSON。
DTO/OpenAPI 保存在 docs/api，前端类型由 `pnpm run api:generate` 生成。

## 门禁与部署

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/check-architecture.sh
cd web && pnpm run api:check && pnpm run check && pnpm run build
```

架构检查需要 cargo-deny、Python 3.11+ 与 ripgrep；公告检查需要网络。
浏览器验证先在 web 运行 `pnpm exec playwright install chromium`，Linux CI 使用 --with-deps。
实际数据库和部署验证见 [验证记录](docs/verification.md)。

生产主路径：`provision-debian.sh` 初始化 → 构建 → `activate-release.sh` 校验版本、
原子切换、迁移、逐个重启与回滚。源码与编译缓存留在 src，发布包只含独立运行产物。
2 GB 主机使用 CARGO_JOBS=1 和 swap；新增媒体依赖后旧骨架的编译开销不能代表当前版本。
详细 Nginx TLS、systemd、双 bucket 与恢复步骤见 [部署手册](docs/deployment.md)。

CI 包含 Rust/前端/契约/依赖门禁、Postgres 16/17/18 集成验证、浏览器 a11y/SEO、
部署回滚、真实 Nginx 分流、逻辑恢复和隔离 PITR。没有部署 Caddy 的依赖或步骤。
本地通过与远程 GitHub Actions、真实 Debian VPS、线上 R2/Cloudflare 的验收是不同证据；
已执行结果及限制如实记录在 verification.md。

## 许可证

本项目以 [MIT](LICENSE) 分发；Cargo workspace 与前端统一声明 MIT。
第三方依赖保留各自许可条款，按 [deny.toml](deny.toml) 的策略检查全部 workspace、
功能、开发/构建及传递依赖，不跳过 publish=false 的内部 crate：

```bash
cargo deny --workspace --all-features --locked check licenses
./scripts/check-architecture.sh
```
