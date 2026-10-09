# 本地开发指南

本指南从“只安装了工具”开始，直到浏览器能登录和写文章。
以下命令使用 Bash，工作目录为仓库根目录；前端命令明确标注 `cd web`。
生产主机使用 [部署手册](deployment.md)，不要在开发机器运行 Debian 初始化脚本。
开始开发前切换到 `dev`；后续更改提交到 `dev`，所有分支保持线性历史。
提交与维护者批准发布的完整流程见 [贡献指南](../CONTRIBUTING.md)。

## 1. 安装并确认工具

| 工具 | 要求 | 检查 |
|---|---|---|
| Rust | ≥ 1.94.1，含 rustfmt/clippy | `cargo --version` |
| Node | ≥ 22.12，推荐 Node 22/24 LTS | `node --version` |
| pnpm | 12.9.1 | `pnpm --version` |
| PostgreSQL | ≥ 16，**包含服务端** initdb/pg_ctl，不只是 psql | `pg_config --version` |
| 原生构建 | C 工具链、pkg-config、CMake | `cc --version`、`cmake --version` |
| 验证工具 | Python ≥ 3.11、ripgrep、cargo-deny；Linux util-linux | `python3 --version`、`rg --version` |

确认当前终端的 Node 版本后安装 pnpm：`npm install --global pnpm@12.9.1`。
切换 Node 版本后重新打开终端，并在 `web` 运行 `pnpm exec node --version`，
确认前端实际使用的版本；系统 Node 20 不满足本项目要求。

Arch / EndeavourOS 可安装：

```bash
sudo pacman -Syu --needed postgresql base-devel cmake pkgconf python ripgrep
```

Debian / Ubuntu 先安装构建工具，再安装版本 ≥ 16 的 PostgreSQL 服务端。
Debian 12 默认数据库大版本不满足要求，使用 [PostgreSQL 官方 APT 源](https://www.postgresql.org/download/linux/debian/)；
按照官方步骤配置源后，例如：

```bash
sudo apt install build-essential pkg-config cmake python3 ripgrep postgresql-18 postgresql-client-18
```

本地无需中文扩展或对象存储即可写文章；缺少中文扩展时使用应用分词。
本地不需要 Nginx，Astro 自带同源开发代理。

## 2. 启动项目专用数据库

安装包不等于服务已启动。推荐使用项目脚本，避免依赖系统 socket、系统 postgres
账号或默认 5432 端口。以当前普通用户执行：

```bash
./scripts/dev-db.sh start
./scripts/dev-db.sh status
./scripts/dev-db.sh url
```

成功时显示 `本地数据库 sloray 就绪：127.0.0.1:55432`。
`url` 输出真实随机密码的连接串，将其粘贴到安装向导，勿提交或公开它。
脚本自动创建 UTF8 数据库；无需再执行 `createdb sloray`。

| 项目 | 默认值 |
|---|---|
| 地址 | `127.0.0.1:55432`，仅回环 TCP |
| 角色 / 库 | `sloray` / `sloray` |
| 数据目录 | `.dev-cache/postgres/data/` |
| 日志 | `.dev-cache/postgres/postgres.log` |
| 密码文件 | `.dev-cache/postgres/password`，0600 |
| 私有 socket | `.dev-cache/postgres/socket/`，父目录 0700 |

这是开发专用集群，角色具有初始化集群的管理权限；生产使用权限受限的应用角色。
TCP 使用随机密码和 scram-sha-256；本地 socket 位于开发用户独占目录。
重复 start 复用已有集群、端口和密码；stop 后数据保留。

```bash
./scripts/dev-db.sh stop
./scripts/dev-db.sh start
```

默认端口被占用时，首次启动或停止后指定其它端口：

```bash
SLORAY_DEV_DB_PORT=55433 ./scripts/dev-db.sh start
./scripts/dev-db.sh url
```

变更端口后在 `/setup` 救急通道或已登录的设置页更新连接串。
`SLORAY_DEV_DB_DIR` 可选择另一私有目录，后续每次操作都要指定相同目录。
Debian 的服务端命令可能不在 PATH；脚本会查找 `/usr/lib/postgresql/{18,17,16}/bin`。
多版本机器可指定 `PG_BIN=/usr/lib/postgresql/17/bin`，后续继续用同一大版本。
升级 PostgreSQL 大版本需要迁移数据，不能把旧目录直接交给新 initdb/pg_ctl。

如果已有系统或远程数据库，可以跳过脚本，确认连接串能实际连接并且应用角色
拥有目标库、有建表权限；这时不要把项目 55432 的连接串用于系统 5432。

## 3. 启动 API、前端并初始化

推荐在第 2 节启动数据库后，用一个终端管理全部开发进程：

```bash
cd web
pnpm install --frozen-lockfile
cd ..
./scripts/dev-start.sh
```

脚本构建 Rust 二进制，启动独立 API 和 Astro，API 的 `/readyz` 成功后再启动持续
运行的 worker。首次访问 `http://127.0.0.1:4321/setup` 完成下面的初始化；等待就绪
期间前端和 API 保持可用。另开终端用脚本显示的**同一份配置路径**读取 setup token：

```bash
SLORAY_CONFIG_FILE='<启动脚本显示的绝对配置路径>' cargo run --locked --bin sloray -- setup-token
./scripts/dev-db.sh url
```

默认使用根目录 `.dev-cache/runtime/config.json`；如果仅有之前在 `web` 下创建的
`web/.dev-cache/runtime/config.json`，直接复用它，不复制配置或创建新的站点。
两份配置同时存在时必须用 `SLORAY_CONFIG_FILE` 明确选择；显式相对路径以调用脚本
时的目录为准。API 和 worker 共用这个绝对路径，Rust 命令在仓库根目录运行，
因此也共用根目录 `.env` 的覆盖规则。

日志追加到 `.dev-cache/dev/{api,web,worker}.log`，新日志权限为 0600。
Ctrl+C 停止本次启动的全部进程及子进程；服务意外退出也会停止其余进程并提示日志路径。
端口占用时拒绝启动，不终止旧开发终端；先停止之前手动启动的 API / Astro 再执行脚本。
数据库由第 2 节单独管理，脚本退出不会停库。修改数据库、S3 或 CDN 设置后重启
脚本，因为持续运行的 worker 在启动时读取配置。

也可以按以下方式用三个独立终端启动。

**终端 A，仓库根目录**：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- serve
```

API 默认监听 `127.0.0.1:8080`。首次 Rust 编译需要等待完成；看到服务监听日志后，
另开终端确认 `curl -f http://127.0.0.1:8080/healthz` 成功。
未初始化时 `/readyz` 返回 503 是预期行为。

**终端 B，仓库根目录**：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- setup-token
./scripts/dev-db.sh url
cd web
pnpm install --frozen-lockfile
pnpm run dev
```

保存 token 和连接串，在浏览器打开 **http://127.0.0.1:4321/setup**。
填入两者、管理员邮箱/密码和站点地址 `http://127.0.0.1:4321`，媒体字段可先留空。
提交时测试连接、执行迁移、创建管理员、保存私有配置并登录。
之后首页和 `/admin/posts` 可用；`curl -f http://127.0.0.1:4321/readyz` 应返回 200。

前端和 API 必须同时保持运行。统一使用 `127.0.0.1:4321` 浏览和登录，
不要在 localhost、127.0.0.1 或不同端口间混用登录 Cookie。
Astro dev 的 `/api`、`/login`、`/logout`、`/setup`、`/admin/settings` 与探针请求
代理到 API，其它页面由 Astro SSR 渲染。不要把 API 8080 当作完整前端地址。

**终端 C，仓库根目录（媒体、定时发布及 outbox 必需）**：

```bash
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- worker
```

worker 负责定时发布、媒体派生和 outbox；完成初始化后启动，使用与 serve 完全相同的配置路径。
仅运行 serve 不会执行这些任务：原图可成功进入私有桶，但不会生成公开 WebP。
关闭开发环境时在三个终端按 Ctrl+C，再执行 `./scripts/dev-db.sh stop`。
Astro 7 若被显式或代理工具以后台方式启动，在 web 使用
`pnpm exec astro dev status`、`pnpm exec astro dev logs` 和 `pnpm exec astro dev stop` 管理。

## 4. 自定义端口与配置

统一启动脚本的自定义端口示例（两个端口均只监听回环）：

```bash
SLORAY_DEV_API_PORT=18080 SLORAY_DEV_WEB_PORT=14321 ./scripts/dev-start.sh
```

API 端口变化时，两侧使用一致地址：

```bash
# 终端 A：根目录
SLORAY_CONFIG_FILE="$PWD/.dev-cache/runtime/config.json" cargo run --locked --bin sloray -- serve --bind 127.0.0.1:18080
# 终端 B：web 目录
SLORAY_API_INTERNAL_URLS=http://127.0.0.1:18080 pnpm run dev --port 14321
```

浏览器访问 `http://127.0.0.1:14321/setup`，站点地址也填相同 URL。
开发代理使用 SLORAY_API_INTERNAL_URLS 第一项，SSR 使用同一列表进行只读故障切换。
Nginx 生产 upstream、systemd 的 API 实例名与开发参数互不覆盖。

配置优先级为默认值 → JSON → 进程环境变量。`serve`、`setup-token`、worker 和迁移
必须指向同一 config.json；从其它目录运行时优先指定绝对路径。
本地默认私有目录位于被 Git 忽略的 `.dev-cache/`，避免默认 `/srv/sloray/shared/` 的权限问题。
不要删掉该目录来“修复启动”：里面包含已有文章、账号、连接串和 token。

根目录 `.env.example` 是可选模板。**sloray 根 CLI 会自动加载当前目录或父目录的 .env**，
不覆盖进程里已有的环境变量；复制并编辑后即可使用：

```bash
cp .env.example .env
# 编辑 .env；如覆盖数据库地址，填写 dev-db.sh url 的实际输出
```

环境变量优先于向导设置：进程或旧 .env 中残留 SLORAY_DATABASE_URL 会让修改连接串
看起来无效。旧模板的 sloray:sloray@127.0.0.1:5432 示例不是自动创建的凭据，
应删除该覆盖或改为 dev-db.sh url 的真实值；修改后重启 API/worker。
Astro 的 web/.env 由开发工具加载；数据库密码和 S3 密钥不需要放进前端环境，
更不能使用会注入浏览器的 PUBLIC_ 前缀。

## 5. 本地验证

贡献门禁见 [AGENTS.md](../AGENTS.md)。开发路径新增两个真实服务检查：

```bash
python3 scripts/test-dev-db.py
python3 scripts/test-dev-start.py
cargo build --locked --bin sloray
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-dev.py
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-dev-start.py
pnpm --dir web run build
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-pages.py
SLORAY_TEST_DATABASE_URL="$(./scripts/dev-db.sh url)" python3 scripts/smoke-browser.py
```

test-dev-db 使用临时隔离集群并在结束停止，不修改当前开发库。
test-dev-start 使用临时目录、模拟业务进程及真实 HTTP/进程组，验证共用配置、等待就绪、
Ctrl+C / 进程失败清理、端口冲突和配置歧义；不连接开发数据库或对象存储。
smoke-dev 创建/删除独立测试库，检查真实 Astro dev 页面、Markdown、API/登录/设置代理，
而不是仅检查端口或生产 build。测试用户须能创建数据库；项目开发角色已具备权限。
smoke-dev-start 使用同样的隔离数据库及本地模拟对象存储，实际执行统一启动脚本，
确认真实 Astro/API 代理、worker 自动生成 WebP 与停止后释放端口。
另外的独立 SSR、浏览器、恢复测试与实际结果见 [verification.md](verification.md)。
页面测试覆盖管理员权限、CSRF、版本、发布与撤回、导航、sitemap 和预览清洗；
浏览器测试覆盖媒体选择器上传/复用/剪贴板、Markdown 预览和后台页面操作。使用说明见 [管理后台](admin.md)。

## 6. 常见错误

| 现象 | 原因与处理 |
|---|---|
| createdb 找不到 `/run/postgresql/.s.PGSQL.5432` | 默认 socket 上没有可用服务；createdb 不会启动 PostgreSQL。先执行 dev-db.sh start，使用 url 输出的 TCP 55432 地址。若使用既有服务，先验证其实际地址、端口和运行状态 |
| `initdb` 找不到 | 只安装了客户端，或服务端 bin 不在 PATH；安装 PostgreSQL 服务端，必要时指定 PG_BIN |
| `role "<当前用户名>" does not exist` | psql/createdb 未指定用户会使用当前系统用户名；使用完整连接串或明确 `-h/-p/-U` |
| `password authentication failed` | 用错端口/集群或密码；重新取 dev-db.sh url，检查环境变量覆盖；不要重初始化已有数据目录 |
| `address already in use` | 对应端口已被其它进程占用；检查现有服务，或按第 2/4 节更换端口，不终止未知进程 |
| API 健康但 readyz=503 | 未初始化、数据库未启动或连接串不对；打开 /setup 或检查日志；healthz 只证明进程存活 |
| 原图上传成功但一直未就绪，公开桶没有图片 | worker 未运行、用错配置或投递失败；使用 dev-start.sh 启动全部进程，检查 .dev-cache/dev/worker.log。修改存储设置后重启 worker |
| `Failed to create the dev server app: require is not defined` | 旧配置将全量 noExternal 用于开发模式，导致 CommonJS 被当作 ESM 执行；当前修复只在 build 启用全量打包。停止旧 dev 进程，拉取修复，确认 Node ≥22.12，然后重新安装锁定依赖并启动 |
| Astro 有端口但页面报错/无响应 | 查看 dev 终端或 astro dev logs；必须实际请求 /about 和 /setup，不能以启动提示代替验证 |
| 点击登录/保存时代理连接失败 | API 未启动或代理端口不一致；从 4321/readyz 验证，检查 SLORAY_API_INTERNAL_URLS |

开发模式必须让 Node 正常加载 CommonJS；发布模式仍打包依赖以便只携带 dist 运行。
相关语义见 [Vite SSR 配置](https://vite.dev/config/ssr-options#ssr-noexternal)，
CLI 管理见 [Astro 官方说明](https://docs.astro.build/en/reference/cli-reference/#astro-dev)。
