#!/usr/bin/env bash
# 项目独享的本地 PostgreSQL；不修改系统服务或既有数据库。
set -euo pipefail
umask 077

die() { printf '错误：%s\n' "$*" >&2; exit 1; }
REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
DB_DIR="${SLORAY_DEV_DB_DIR:-$REPO/.dev-cache/postgres}"
DB_PORT="${SLORAY_DEV_DB_PORT:-55432}"
ACTION="${1:-start}"
[[ $# -le 1 ]] || die "用法：$0 start|stop|status|url"
[[ "$ACTION" =~ ^(start|stop|status|url)$ ]] || die "用法：$0 start|stop|status|url"
[[ $EUID -ne 0 ]] || die "本地数据库请以普通开发用户运行，不要使用 sudo"
[[ "$DB_PORT" =~ ^[0-9]{1,5}$ ]] || die "SLORAY_DEV_DB_PORT 必须是端口数字"
DB_PORT=$((10#$DB_PORT))
((DB_PORT > 0 && DB_PORT < 65536)) || die "端口必须在 1–65535 之间"

if [[ -z ${PG_BIN:-} ]]; then
  if command -v initdb >/dev/null 2>&1; then
    PG_BIN="$(dirname -- "$(command -v initdb)")"
  else
    for major in 18 17 16; do
      if [[ -x /usr/lib/postgresql/$major/bin/initdb ]]; then
        PG_BIN="/usr/lib/postgresql/$major/bin"
        break
      fi
    done
  fi
fi
[[ -n ${PG_BIN:-} ]] || die "缺少 PostgreSQL 服务端工具 initdb；安装方法见 docs/development.md"
for tool in initdb pg_ctl psql createdb; do
  [[ -x "$PG_BIN/$tool" ]] || die "PG_BIN 中缺少 $tool；请安装同一版本的服务端和客户端"
done
PG_MAJOR="$("$PG_BIN/initdb" --version | sed -E 's/.* ([0-9]+)\..*/\1/')"
if [[ ! "$PG_MAJOR" =~ ^[0-9]+$ ]] || ((PG_MAJOR < 16)); then
  die "需要 PostgreSQL 16 或更新版本"
fi
command -v flock >/dev/null 2>&1 || die "缺少 flock（Linux 的 util-linux 包）"

[[ ! -L "$DB_DIR" ]] || die "数据目录不能是符号链接"
mkdir -p -- "$DB_DIR"
DB_DIR="$(cd -- "$DB_DIR" && pwd -P)"
chmod 0700 "$DB_DIR"
exec 9>"$DB_DIR/control.lock"
flock 9
DATA="$DB_DIR/data"
SOCKET="$DB_DIR/socket"
PASSWORD="$DB_DIR/password"
[[ ! -L "$DATA" && ! -L "$SOCKET" && ! -L "$PASSWORD" ]] || die "数据库文件不能是符号链接"
if [[ -f "$DATA/PG_VERSION" && "$(cat "$DATA/PG_VERSION")" != "$PG_MAJOR" ]]; then
  die "已有数据为 PG $(cat "$DATA/PG_VERSION")，请设置 PG_BIN 使用相同大版本；不能原地升级"
fi

running() { "$PG_BIN/pg_ctl" -D "$DATA" status >/dev/null 2>&1; }
active_port() {
  [[ -f "$DB_DIR/port" ]] || die "缺少端口记录；请先 start"
  DB_PORT="$(cat "$DB_DIR/port")"
  [[ "$DB_PORT" =~ ^[0-9]{1,5}$ ]] || die "端口记录损坏"
}
sql() {
  PGPASSWORD="$(cat "$PASSWORD")" "$PG_BIN/psql" -h 127.0.0.1 -p "$DB_PORT" \
    -U sloray -d postgres -v ON_ERROR_STOP=1 -tAc "$1"
}

case "$ACTION" in
  start)
    if [[ -z ${SLORAY_DEV_DB_PORT+x} && -f "$DB_DIR/port" ]]; then
      active_port
    fi
    if [[ ! -f "$DATA/PG_VERSION" ]]; then
      [[ ! -d "$DATA" ]] || die "初始化未完成，保留目录供排错：$DATA"
      mkdir -p -- "$SOCKET"
      [[ -f "$PASSWORD" ]] || od -An -N24 -tx1 /dev/urandom | tr -d ' \n' >"$PASSWORD"
      chmod 0600 "$PASSWORD"
      "$PG_BIN/initdb" -D "$DATA" --username=sloray --encoding=UTF8 --no-locale \
        --auth-local=trust --auth-host=scram-sha-256 --pwfile="$PASSWORD" >/dev/null
      # Unix socket 只在 0700 私有目录；TCP 仅回环并要求随机密码。
      socket_config="${SOCKET//\'/\'\'}"
      printf "\nlisten_addresses='127.0.0.1'\nunix_socket_directories='%s'\nmax_connections=30\n" \
        "$socket_config" >>"$DATA/postgresql.conf"
    fi
    [[ -s "$PASSWORD" ]] || die "缺少数据库密码文件，不能接管已有数据"
    if running; then
      active_port
      printf '本地 PostgreSQL 已运行，复用端口 %s\n' "$DB_PORT"
    else
      printf '%s\n' "$DB_PORT" >"$DB_DIR/port"
      "$PG_BIN/pg_ctl" -D "$DATA" -l "$DB_DIR/postgres.log" -w -t 30 \
        -o "-p $DB_PORT" start 9>&- || die "启动失败：检查端口占用与 $DB_DIR/postgres.log"
    fi
    if [[ "$(sql "SELECT 1 FROM pg_database WHERE datname='sloray'")" != 1 ]]; then
      PGPASSWORD="$(cat "$PASSWORD")" "$PG_BIN/createdb" -h 127.0.0.1 -p "$DB_PORT" \
        -U sloray --encoding=UTF8 --template=template0 sloray
    fi
    printf '本地数据库 sloray 就绪：127.0.0.1:%s\n连接串：./scripts/dev-db.sh url\n' "$DB_PORT"
    ;;
  stop)
    if running; then
      "$PG_BIN/pg_ctl" -D "$DATA" -m fast -w -t 30 stop
    else
      printf '本地 PostgreSQL 未运行；数据保留在 %s\n' "$DATA"
    fi
    ;;
  status)
    if running; then
      active_port
      sql 'SELECT version()'
    else
      die "本地 PostgreSQL 未运行；执行 ./scripts/dev-db.sh start"
    fi
    ;;
  url)
    running || die "本地 PostgreSQL 未运行；请先 start"
    active_port
    [[ -s "$PASSWORD" ]] || die "缺少密码文件"
    printf 'postgres://sloray:%s@127.0.0.1:%s/sloray\n' "$(cat "$PASSWORD")" "$DB_PORT"
    ;;
esac
