# 部署手册

目标：Debian 12/13，Nginx + 两个 Rust API 实例 + 一个 Astro SSR 进程，
Postgres 保存业务状态，R2 保存媒体。编译在目标机器进行，CI 负责质量门。
代理选型见 [ADR-0013](adr/0013-nginx-reverse-proxy.md)。

本地运行和排错使用 [开发指南](development.md)。本手册中的 sudo、系统服务、
防火墙和 `/srv/sloray` 操作只用于准备好的 Debian 目标主机。
首次部署按以下顺序完成，每一步确认后再进入下一步：

| 步骤 | 完成标志 |
|---|---|
| 主机初始化与数据库 | 工具版本符合要求；Postgres 的实际端口可连接，应用角色拥有数据库 |
| 构建并激活首个版本 | API 的 8080/8081 healthz 为 200，Web 4321 可响应 |
| SSH 隧道或已配置的 HTTPS 向导 | 创建管理员并保存配置，两个 API 的 readyz 均为 200 |
| Nginx 域名与 TLS | nginx -t 成功，HTTPS 下首页、登录与 API 均可用 |
| worker 与备份 | timer 正常，定时发布/媒体任务可完成；备份和恢复路径已验收 |

首次未初始化时 readyz=503 是正常状态，不能以 healthz=200 代替数据库已就绪。

## 1. 初始化主机

要求 Rust ≥ 1.94.1、Node ≥ 22.12、pnpm 12.9.1、Postgres ≥ 16。
2 vCPU / 2 GB 主机使用 `CARGO_JOBS=1` 并启用至少 2 GB swap。
构建包含对象存储 SDK 与图像库；旧骨架的内存/耗时不能代表当前版本。建议至少预留 10 GB 可用磁盘；源码、Cargo 缓存和前端依赖不进入版本目录。

```bash
scp -r deploy/scripts root@<vps>:/tmp/sloray-scripts
ssh root@<vps>
sudo /tmp/sloray-scripts/provision-debian.sh
swapon --show
ufw status
```

初始化脚本安装系统包、Postgres、Rust、Node、pnpm、Nginx，创建 `sloray`
系统用户与数据库。防火墙只开放 22/80/443，API 和 Astro 始终监听回环地址。
数据库连接串写入 `/etc/sloray/api.env`（0640 root:sloray），
其余配置通过浏览器向导完成。重跑不会改动已有密码或连接串；已有同名角色却没有 api.env 时拒绝自动接管，需先由运维配置凭据。服务单元固定使用 sloray 用户。部署脚本不会安装第二个反向代理。

脚本针对 Debian；本地隔离测试不等同于已经在真实 Debian VPS 执行过初始化。

## 2. 数据库

数据库角色必须拥有应用数据库，认证使用 scram-sha-256。
安装数据库客户端或运行 createdb 都不会启动数据库服务。初始化后先确认实例：

```bash
pg_lsclusters
pg_isready -h 127.0.0.1 -p 5432
sudo -u postgres psql -p 5432 -d postgres -c 'SHOW server_version;'
sudo -u postgres psql -p 5432 -d postgres -c 'SHOW config_file;'
sudo -u postgres psql -p 5432 -d postgres -c "SELECT datname, pg_get_userbyid(datdba) AS owner FROM pg_database WHERE datname='sloray';"
```

表中应看到 online 的集群、实际版本 ≥16 和 sloray 数据库的 owner=sloray。
多集群机器以 pg_lsclusters 的实际大版本、集群名和端口为准；上述 5432 是默认值。
例如 17/main 状态为 down 时，可执行 `sudo pg_ctlcluster 17 main start`，再检查日志。
`systemctl status postgresql` 的汇总单元可能显示 active (exited)，仍需确认具体集群。
`/etc/sloray/api.env` 的连接串端口必须与目标实例一致；原有配置重跑时不会自动更改。
不要运行裸 createdb 来猜测默认角色或实例，更不要用开发脚本管理生产数据。

根据 SHOW config_file 的实际路径调整配置，不要把 17/main 路径套到其它大版本。
按主机内存调整参数，不要直接套用服务器参数。
2 GB 主机可从 `shared_buffers=384MB`、`work_mem=8MB`、
`max_connections=50` 起步；两个 API 连接池默认各最多 10 条连接。

```bash
sudo /tmp/sloray-scripts/install-pg-jieba.sh
sudo -u postgres psql -d sloray -tAc \
  "SELECT to_tsvector('jiebacfg','分布式博客的架构设计');"
```

扩展为可选步骤，安装时必须确认中文被分词。没有可用扩展时迁移自动使用应用分词，详见 [search.md](search.md)。不要改全局 `default_text_search_config`。
迁移不在普通 `serve` 启动时执行；向导或发布流程单独执行，并使用咨询锁。

## 3. 首次编译与发布

生产部署使用维护者批准并已从 `dev` 快进到 `main` 的版本标签。
版本递增、最新 `dev` 完整 CI 与一次快进的顺序见 [贡献指南](../CONTRIBUTING.md)。
下例中的 `<批准的版本号>` 必须替换为实际已批准的版本，不带 `v`。

```bash
sudo git clone --branch main <repo> /srv/sloray/src
sudo CARGO_JOBS=1 /srv/sloray/src/deploy/scripts/deploy-on-vps.sh 'v<批准的版本号>'
```

部署前确认源码是已提交的版本，发布脚本拒绝脏目录。确认 `node --version` ≥22.12、
`pnpm --version` 为 12.9.1；sudo 的 PATH 可能与登录终端不同，初始化脚本准备的
Rust 位于 `/usr/local/cargo/bin`，发布脚本会显式使用它。
前端发布执行 build，systemd 运行 `dist/server/entry.mjs`；发布产物无需 node_modules。
Astro 开发模式由 Node 加载依赖，生产 build 才启用完整打包，两个路径均有 CI 冒烟测试。

构建使用 Rust 锁文件与 pnpm 冻结锁文件。`activate-release.sh` 校验产物，
拒绝覆盖已存在版本，原子切换 `current`，迁移后逐个重启 API，再重启 Web。
首次没有数据库配置时跳过迁移，仅检查 `/healthz`，让安装向导先启动。拒绝覆盖未提交源码，指定 ref 使用 detached checkout；staging 必须直接位于发布根目录，不能为保留目录或符号链接。

```
/srv/sloray/
├── current -> releases/<version>
├── releases/<version>/{bin,web/dist,deploy,docs}
├── src/                  # 源码、target、node_modules
└── shared/               # 0700 sloray:sloray
    ├── config.json       # 0600；运行时配置
    └── setup-token       # 0600；部署凭据
/etc/sloray/api.env        # 可选的运维覆盖，0640 root:sloray
/etc/sloray/web.env        # 可选，0640 root:sloray
```

systemd 单元引用稳定的 `current` 路径。发布失败会恢复 symlink 和旧单元，
重新加载 systemd 并启动旧服务。首次发布失败则停止新服务。worker.service/timer 随版本安装；迁移前暂停 timer 与正在运行的 worker，成功或回滚后恢复。正常使用 activate-release.sh，不单独调用 rollout.sh。
数据库 schema 不回滚，因此所有发布迁移必须遵守 expand / contract。

## 4. 安装向导

```bash
sudo -u sloray /srv/sloray/current/bin/sloray setup-token
```

本命令使用默认 `/srv/sloray/shared/config.json`。自定义根目录部署时显式使用
`--config <实际 shared/config.json 路径>`，与服务的 EnvironmentFile 和单元路径保持一致。
Rust CLI 会加载当前目录或父目录的 .env，但不会自动读取 `/etc/sloray/api.env`；
systemd 的 EnvironmentFile 为服务注入运维配置，已有环境变量优先。
生产发布目录不放开发 .env。复制连接串时从受限环境文件取得实际值，不能照抄示例密码。

主机只开放代理端口，不能直接访问 `http://<vps>:8080`。
TLS/域名尚未配置时，从本机建立 SSH 隧道：

```bash
ssh -L 18080:127.0.0.1:8080 <user>@<vps>
# 浏览器打开 http://127.0.0.1:18080/setup
```

填写 token、数据库连接串、管理员、站点地址。数据库/角色必须已经创建，
建表由向导完成。有数据库连接串但还没有管理员时，向导仍然可用。
S3 可先留空，之后在 `/admin/settings` 中补齐。原图与公开派生图必须使用不同 bucket，配置见 [media.md](media.md)。

配置与连接池在验证及成功落盘后一起切换，另一实例每两秒读取共享配置。
启动时数据库不可达会重试连接；未配置的 `/healthz` 为 200，`/readyz` 为 503。
正常初始化后向导关闭。保留的 token 只在未初始化或数据库不可达时接受；
运行 `setup-token --rotate` 可以轮换它，即使 config.json 已损坏也能执行。损坏 JSON 本身会阻止 serve 启动，需先保留副本并修复文件；数据库暂不可达可使用向导恢复。站点 canonical/RSS/sitemap 的 SSR 值从运行时设置读取，about/404 等预渲染页面仍使用构建地址。

环境变量优先于配置文件，因此运维覆盖与页面设置应保持一致；
删除过时的 `api.env` 覆盖后重启服务。S3 的扁平变量映射到嵌套配置：
`SLORAY_S3_ENDPOINT`、`SLORAY_S3_REGION`、`SLORAY_S3_BUCKET`、
`SLORAY_S3_ACCESS_KEY_ID`、`SLORAY_S3_SECRET_ACCESS_KEY`。

## 5. Nginx 与 TLS

证书签发、续期使用目标主机现有的 ACME/TLS 流程。
仓库提供 `deploy/nginx/sloray.conf`，在 `http` 上下文加载。
替换其中域名和证书路径后：

```bash
sudo install -m 0644 /srv/sloray/src/deploy/nginx/sloray.conf /etc/nginx/sites-available/sloray
sudo ln -s /etc/nginx/sites-available/sloray /etc/nginx/sites-enabled/sloray
sudo nginx -t
sudo systemctl reload nginx
curl -f https://blog.example.com/healthz
curl -f https://blog.example.com/readyz
```

如果同名 sites-enabled 文件已存在，先确认它指向当前配置再重载，不重复创建链接。
从 http 跳转 HTTPS 应保留路径和查询串；完成 TLS 后，运行时站点地址应为实际
`https://<域名>`，并重新登录以使用 Secure Cookie。

API、探针、安装/登录/登出、后台设置到 Rust；内容页面和后台写作到 Astro。
Nginx 覆盖 `X-Real-IP` 与转发协议，应用不能无条件相信来自任意连接的同名头。
媒体域名直接指向 R2/CDN，媒体字节不经 Nginx 或 API。

开源 Nginx 使用被动故障检测。GET/HEAD 在连接失败和 502/503/504 时可以尝试
另一个 API。未开启 `non_idempotent`，已经发送的 POST/PATCH 不会自动重放。
因此逐个重启降低中断，但不保证所有写请求零失败。Web 是单实例，重启期间
存在短暂的请求失败窗口。SSR 直连 API 也必须验证故障切换。

动态页面默认不在 Nginx 层缓存；后台、探针和错误响应使用 `no-store`。
`/_astro/` 的真实资源由 Astro 设置 immutable，缺失资源不能强加长期缓存头。
若启用 CDN 动态缓存，需要配套验证发布、修改、撤回和删除的 purge。
应用发布不会覆盖主机上的 Nginx 配置和 TLS 证书。

## 6. 后续发布、回滚与离线安装

```bash
sudo CARGO_JOBS=1 /srv/sloray/src/deploy/scripts/deploy-on-vps.sh v0.2.0
sudo /srv/sloray/src/deploy/scripts/install-release.sh /tmp/sloray-v0.2.0.tar.gz
```

`rollout.sh` 通过 transient systemd 服务读取与 API 相同的 EnvironmentFile，
执行一次带咨询锁的迁移，等待 8080 就绪后才重启 8081。
首次安装尚未配置时检查进程存活；已配置发布必须检查数据库就绪。

迁移纪律：只增加可空列/索引 → 新代码切换 → 下一个稳定版本移除旧结构。
不要在同一个发布里删旧列并依赖 `down` 救场。

`cargo install --path . --root /usr/local` 只安装 Rust 二进制；
要有完整站点仍需构建 Astro、安装 Nginx 配置并准备服务用户和共享目录。
`sloray systemd print` 可先检查嵌入模板；`install --no-enable --dir <path>`
可在隔离目录只渲染文件。使用正常 `systemd install` 前须完成主机初始化。

## 7. 备份与恢复

`pg_dump -Fc` 是逻辑全量备份，不能单独提供 PITR。
需要 PITR 时用 pgBackRest 或连续 WAL 归档，并记录恢复点验证。
`backup-postgres.sh` 负责逻辑转储、目录可读检查、SHA-256、上传及保留期清理；远端失败时保留旧备份。完整步骤与 PITR 演练见 [backup.md](backup.md)。
备份账号与对象存储凭据使用主机的受限环境文件，备份文件只对管理员开放。

每季度在独立数据库执行恢复演练；不能对生产库运行 `migrate fresh`。
`pg_restore --list` 只证明目录可读，不能证明所有数据可恢复。

## 8. 验证与排错

```bash
systemctl status sloray@8080 sloray@8081 sloray-web sloray-worker.timer nginx
journalctl -u sloray@8080 -f
sudo nginx -t
curl -f http://127.0.0.1:8080/healthz
curl -f http://127.0.0.1:8080/readyz
```

数据库问题按“集群 → 地址/端口 → 角色/密码 → 应用环境覆盖”的顺序检查：

```bash
pg_lsclusters
pg_isready -h 127.0.0.1 -p 5432
systemctl status postgresql@17-main
journalctl -u postgresql@17-main -n 50
```

将示例 17-main 替换为实际集群。找不到 `/run/postgresql/.s.PGSQL.5432` 时，
先确认是否启动了正确集群或是否本来使用 TCP/其它端口，不重建已有数据库。
连接正常但应用仍未就绪，检查 API 日志以及 `/etc/sloray/api.env` 中旧的
SLORAY_DATABASE_URL 覆盖；环境变量比 config.json 优先。勿将含凭据的日志/配置公开粘贴。

完整本地门禁：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-architecture.sh
cd web && pnpm run check && pnpm run build
```

部署与端到端测试：

```bash
deploy/scripts/test-activate-release.sh
deploy/scripts/test-rollout.sh
python3 deploy/scripts/test-release-package.py
deploy/scripts/test-backup.sh
python3 scripts/test-nginx.py
SLORAY_TEST_DATABASE_URL=postgres://... python3 scripts/smoke-setup.py
```

Nginx 测试使用临时 TLS 证书和回环端口，验证真实分流、头覆盖与失败切换。
setup 测试为每次运行创建并删除独立数据库，验证并发 token、并发初始化、
配置权限与两个 API 实例同步。CI 使用相同命令。

上线前还必须在目标 Debian 主机验证初始化、systemd 沙箱、证书续期、
中文扩展和实际对象存储权限。隔离测试不会替代这些外部环境验证。


其它真实数据库验证命令及实际输出集中在 [verification.md](verification.md)。
CI 运行 setup、内容/SSR、评论/计数、搜索、媒体、逻辑恢复与浏览器测试；数据库
使用 PostgreSQL 18，客户端与服务大版本匹配，PITR 另起隔离集群。
CI 不部署主机，workflow 本身通过 actionlint 静态检查。
