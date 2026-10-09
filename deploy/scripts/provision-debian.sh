#!/usr/bin/env bash
#
# Debian 12 / 13 首次初始化。**幂等**：可以反复执行，不会重复做已经做过的事。
#
# 做这些事：
#   1. 基础包 + 时区 + 自动安全更新
#   2. 创建 sloray 系统用户与目录（/srv/sloray、/etc/sloray）
#   3. 装独立 PostgreSQL（PGDG 源，与机器上已有的 MySQL/Percona 互不干扰）
#   4. 建库建角色，并把随机密码写进 /etc/sloray/api.env
#   5. 装 Node 22 与 Nginx
#   6. 防火墙只放 22/80/443
#
# 不做这些事（需要单独执行，见 docs/deployment.md）：
#   * 中文分词扩展 → deploy/scripts/install-pg-jieba.sh
#   * 数据库调优参数 → docs/deployment.md 第 3 节
#   * 密钥轮换
#
# ⚠️ 本脚本只做了 `bash -n` 语法检查，尚未在真实 Debian 主机上跑过。
#    首次执行前请先读完 docs/deployment.md，并确认你知道每一步在改什么。
set -euo pipefail

SLORAY_USER="${SLORAY_USER:-sloray}"
ROOT="${SLORAY_ROOT:-/srv/sloray}"
ETC="${SLORAY_ETC:-/etc/sloray}"
PG_MAJOR="${PG_MAJOR:-17}"
DB_NAME="${DB_NAME:-sloray}"

log() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
die() { printf '\n\033[1;31m错误: %s\033[0m\n' "$*" >&2; exit 1; }

[[ ${EUID} -eq 0 ]] || die "请用 root 执行（sudo $0）"
[[ "$SLORAY_USER" == sloray ]] || die "服务单元固定使用 sloray 用户"
[[ "$DB_NAME" =~ ^[a-zA-Z_][a-zA-Z0-9_]{0,62}$ ]] || die "DB_NAME 必须是合法标识符（最多 63 字节）"
[[ "$PG_MAJOR" =~ ^(16|17|18)$ ]] || die "PG_MAJOR 必须为 16、17 或 18"
[[ -f /etc/debian_version ]] || die "这个脚本只针对 Debian"
log "Debian $(cat /etc/debian_version)"

# ---------------------------------------------------------------- 1. 基础
log "1/7 基础包、时区、自动安全更新"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq \
  ca-certificates curl gnupg lsb-release sudo ufw fail2ban \
  unattended-upgrades apt-listchanges jq rsync git \
  build-essential pkg-config cmake python3 ripgrep >/dev/null

# 服务器一律用 UTC，避免日志与数据库时间需要心算时差。
timedatectl set-timezone UTC 2>/dev/null || true

# 自动装安全更新；不自动重启（重启时机应该由人决定）。
if [[ ! -f /etc/apt/apt.conf.d/20auto-upgrades ]]; then
  cat >/etc/apt/apt.conf.d/20auto-upgrades <<'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
EOF
fi

# ---------------------------------------------------------------- 2. 用户与目录
log "2/7 用户 ${SLORAY_USER} 与目录"
if ! id -u "$SLORAY_USER" >/dev/null 2>&1; then
  # 不给登录 shell：这个账号只用来跑服务，不需要交互登录。
  adduser --system --group --home "$ROOT" --shell /usr/sbin/nologin "$SLORAY_USER"
fi

# 0600：api.env 里有数据库密码和 R2 凭据。
install -d -m 0755 -o root -g root "$ROOT"
install -d -m 0755 -o root -g root "$ROOT/releases"
install -d -m 0700 -o "$SLORAY_USER" -g "$SLORAY_USER" "$ROOT/shared"
install -d -m 0750 -o root -g "$SLORAY_USER" "$ETC"

# ---------------------------------------------------------------- 3. PostgreSQL
log "3/7 PostgreSQL ${PG_MAJOR}（PGDG 源，独立实例）"
PGDG_KEY=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
if [[ ! -d /usr/lib/postgresql/$PG_MAJOR ]]; then
  install -d /usr/share/postgresql-common/pgdg
  curl -fsSL -o "$PGDG_KEY" https://www.postgresql.org/media/keys/ACCC4CF8.asc
  # shellcheck source=/dev/null
  CODENAME="$(. /etc/os-release && echo "$VERSION_CODENAME")"
  echo "deb [signed-by=${PGDG_KEY}] https://apt.postgresql.org/pub/repos/apt ${CODENAME}-pgdg main" \
    >/etc/apt/sources.list.d/pgdg.list
  apt-get update -qq
  # server-dev 是编译 pg_jieba / zhparser 必需的。
  apt-get install -y -qq \
    "postgresql-${PG_MAJOR}" "postgresql-server-dev-${PG_MAJOR}" \
    "postgresql-contrib-${PG_MAJOR}" >/dev/null
else
  log "PostgreSQL ${PG_MAJOR} 已安装，跳过"
fi

systemctl enable --now postgresql

# ---------------------------------------------------------------- 4. 建库建角色
log "4/7 建库与角色"
ENV_FILE="$ETC/api.env"

# 已有部署不自动改密码或连接串，避免重跑时改变正在使用的数据库。
if [[ -f "$ENV_FILE" ]]; then
  grep -q '^SLORAY_DATABASE_URL=' "$ENV_FILE" || die "已有 api.env 缺少连接串，请先修复配置"
  log "已有数据库配置，保持角色、密码与连接串"
else
  DB_PASS="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')"
  # 密码由本脚本生成且只有十六进制；标识符已在任何写操作前校验。
  sudo -u postgres psql -v ON_ERROR_STOP=1 -q <<SQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${SLORAY_USER}') THEN
    CREATE ROLE ${SLORAY_USER} WITH LOGIN PASSWORD '${DB_PASS}';
  ELSE
    RAISE EXCEPTION 'Role already exists without api.env; configure credentials before provisioning';
  END IF;
END
\$\$;
SQL
  if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='${DB_NAME}'" | grep -q 1; then
    sudo -u postgres createdb -O "$SLORAY_USER" "$DB_NAME"
  fi
fi

# 写配置文件（仅当不存在，避免覆盖已有密钥）。
if [[ ! -f "$ENV_FILE" ]]; then
  cat >"$ENV_FILE" <<EOF
# sloray 的生产环境变量。由 provision-debian.sh 首次生成。
# 权限 0640 root:${SLORAY_USER}。**不要提交到版本库。**

SLORAY_ENVIRONMENT=production
SLORAY_LOG_FORMAT=json
# 只监听回环：公网流量一律经 Nginx。
# 实际端口由 systemd 的实例名覆盖（sloray@8080 / @8081）。
SLORAY_BIND_ADDR=127.0.0.1:8080

SLORAY_DATABASE_URL=postgres://${SLORAY_USER}:${DB_PASS}@127.0.0.1:5432/${DB_NAME}
SLORAY_DATABASE_MAX_CONNECTIONS=10
SLORAY_DATABASE_ACQUIRE_TIMEOUT_SECS=5
SLORAY_SHUTDOWN_TIMEOUT_SECS=10

# ---- 下面这些等到对应里程碑再填 ----
# M2 媒体（对象存储）
# SLORAY_S3_ENDPOINT=https://<accountid>.r2.cloudflarestorage.com
# SLORAY_S3_REGION=auto
# SLORAY_S3_BUCKET=sloray-originals
# SLORAY_MEDIA_PUBLIC_BUCKET=sloray-public
# SLORAY_S3_ACCESS_KEY_ID=
# SLORAY_S3_SECRET_ACCESS_KEY=
# SLORAY_MEDIA_PUBLIC_BASE_URL=https://media.example.com
EOF
  chown root:"$SLORAY_USER" "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
  log "已生成 ${ENV_FILE}"

fi

# ---------------------------------------------------------------- 5. swap
log "5/7 swap"
# **2 GB 内存的机器必须有 swap。**
# 实测冷编译 Rust 时 cargo+rustc 的峰值 RSS 是 1.06 GB（-j1）/ 1.35 GB（-j2），
# 再加上 Postgres、Nginx、Node 的常驻内存，没有 swap 兜底就会在链接阶段 OOM，
# 而且 OOM killer 可能顺手把 Postgres 一起杀掉。
if ! swapon --show | grep -q .; then
  if [[ ! -f /swapfile ]]; then
    fallocate -l "${SWAP_SIZE:-2G}" /swapfile || dd if=/dev/zero of=/swapfile bs=1M count=2048
    chmod 600 /swapfile
    mkswap /swapfile >/dev/null
  fi
  swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >>/etc/fstab
  # 降低换出倾向：swap 只作为 OOM 兜底，不希望内核把热数据换出去。
  sysctl -qw vm.swappiness=10
  grep -q '^vm.swappiness' /etc/sysctl.conf || echo 'vm.swappiness=10' >>/etc/sysctl.conf
  log "已启用 swap：$(swapon --show=size --noheadings | head -1)"
else
  log "已有 swap：$(swapon --show=size --noheadings | head -1)"
fi

# ---------------------------------------------------------------- 6. 构建工具链
log "6/7 构建工具链：Rust / Node / pnpm / Nginx"

# --- Rust ---
# 装到 /usr/local，而不是 root 的家目录：发布脚本是以 root 经 sudo 调用的，
# sudo 会重置 PATH，装在 /root/.cargo 下容易出现"交互式能用、脚本里找不到 cargo"。
if ! command -v cargo >/dev/null 2>&1 && [[ ! -x /usr/local/cargo/bin/cargo ]]; then
  # --profile minimal 很重要：默认 profile 会带 rust-docs 等，
  # 在磁盘不大的机器上白占约 700 MB。
  RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo \
      sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path >/dev/null
  cat >/etc/profile.d/rust.sh <<'EOF'
export RUSTUP_HOME=/usr/local/rustup
export CARGO_HOME=/usr/local/cargo
export PATH=/usr/local/cargo/bin:$PATH
EOF
  chmod 0644 /etc/profile.d/rust.sh
fi
if [[ -x /usr/local/cargo/bin/cargo ]]; then
  export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
  export PATH="/usr/local/cargo/bin:$PATH"
fi
log "$(cargo --version)"

# --- Node ---
if ! command -v node >/dev/null 2>&1 || ! node -e 'const [major,minor]=process.versions.node.split(".").map(Number); process.exit(major>22 || major===22 && minor>=12 ? 0 : 1)'; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash - >/dev/null
  apt-get install -y -qq nodejs >/dev/null
fi
log "node $(node -v)"

# --- pnpm ---
# 锁文件和 web/pnpm-workspace.yaml 是用 pnpm 12 生成的，这里钉住大版本。
if ! command -v pnpm >/dev/null 2>&1 || [[ "$(pnpm --version)" != 12.9.1 ]]; then
  npm install -g pnpm@12.9.1 >/dev/null
fi
log "pnpm $(pnpm --version)"

# --- Nginx ---
apt-get install -y -qq nginx >/dev/null
log "Nginx 已安装；域名和 TLS 配置按 docs/deployment.md 完成"

# ---------------------------------------------------------------- 6. 防火墙
log "7/7 防火墙"
ufw default deny incoming >/dev/null
ufw default allow outgoing >/dev/null
ufw allow 22/tcp  >/dev/null
ufw allow 80/tcp  >/dev/null
ufw allow 443/tcp >/dev/null
ufw --force enable >/dev/null
log "ufw: $(ufw status | head -1)"

cat <<EOF

────────────────────────────────────────────────────────
初始化完成。接下来：

1) 配置 Nginx 域名与 TLS（deploy/nginx/sloray.conf 示例）
2) 装上中文分词扩展（首次还没有 current，用仓库里的脚本）：
     sudo /tmp/sloray-scripts/install-pg-jieba.sh
3) 配对象存储（R2）：见 docs/media.md
4) 拉源码并首次发布：
     sudo git clone <你的仓库> /srv/sloray/src
     sudo CARGO_JOBS=1 /srv/sloray/src/deploy/scripts/deploy-on-vps.sh
   ⚠️ CARGO_JOBS=1：2 GB 内存的机器用 -j2 会在链接阶段 OOM。
5) 之后每次发布：
     sudo /srv/sloray/src/deploy/scripts/deploy-on-vps.sh [tag]

数据库连接串已写入 ${ENV_FILE}（0640 root:${SLORAY_USER}）。
资源占用实测数据见 docs/deployment.md。
EOF
