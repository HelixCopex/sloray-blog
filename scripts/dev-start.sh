#!/usr/bin/env bash
# 同一份配置启动独立 API / Astro / worker；只清理本次创建的进程组。
set -euo pipefail
umask 077

die() { printf '错误：%s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || die "用法：$0（配置及端口通过环境变量指定，见 docs/development.md）"
REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
for tool in cargo pnpm python3 curl setsid; do
  command -v "$tool" >/dev/null 2>&1 || die "缺少 $tool；见 docs/development.md"
done
[[ -d "$REPO/web/node_modules" ]] || die "先在 web 运行 pnpm install --frozen-lockfile"

if [[ -z ${SLORAY_CONFIG_FILE:-} ]]; then
  if [[ -f "$REPO/.dev-cache/runtime/config.json" && -f "$REPO/web/.dev-cache/runtime/config.json" ]]; then
    die "根目录和 web 下都有 config.json；请显式指定 SLORAY_CONFIG_FILE，避免使用错误的配置"
  fi
  SLORAY_CONFIG_FILE="$REPO/.dev-cache/runtime/config.json"
  if [[ -f "$REPO/web/.dev-cache/runtime/config.json" ]]; then
    SLORAY_CONFIG_FILE="$REPO/web/.dev-cache/runtime/config.json"
    printf '复用之前在 web 目录创建的配置。\n'
  fi
fi
# 在切换工作目录之前解析调用者传入的相对路径；不复制或读取密钥。
SLORAY_CONFIG_FILE="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$SLORAY_CONFIG_FILE")"
export SLORAY_CONFIG_FILE
API_PORT="${SLORAY_DEV_API_PORT:-8080}"
WEB_PORT="${SLORAY_DEV_WEB_PORT:-4321}"
python3 - "$API_PORT" "$WEB_PORT" <<'PY'
import socket, sys
try:
    ports = [int(value) for value in sys.argv[1:]]
    if len(set(ports)) != 2 or any(not 1 <= value <= 65535 for value in ports):
        raise ValueError
except ValueError:
    sys.exit('错误：API / 前端端口必须是不同的 1–65535 整数')
for name, port in zip(('API', '前端'), ports):
    with socket.socket() as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            listener.bind(('127.0.0.1', port))
        except OSError:
            sys.exit(f'错误：{name} 端口 {port} 不可用；先停止旧开发终端，或指定其它端口。未终止已有进程。')
PY
export SLORAY_API_INTERNAL_URLS="http://127.0.0.1:$API_PORT"
export ASTRO_TELEMETRY_DISABLED=1
LOG_DIR="$REPO/.dev-cache/dev"
mkdir -p -- "$LOG_DIR"
chmod 0700 "$LOG_DIR"
cd "$REPO"
cargo build --locked --bin sloray
# 与 cargo 的标准自定义构建目录保持一致。
BINARY="${CARGO_TARGET_DIR:-$REPO/target}/debug/sloray"
[[ -x "$BINARY" ]] || die "未找到 $BINARY；此脚本使用 Cargo 默认的 debug 构建目录"

PIDS=()
NAMES=()
cleanup() {
  trap '' INT TERM
  local pid
  for pid in "${PIDS[@]}"; do kill -TERM -- "-$pid" 2>/dev/null || true; done
  # 给进程最多约五秒正常退出，再收回残留子进程，避免 pnpm 留下监听端口。
  for _ in {1..25}; do
    local active=0
    for pid in "${PIDS[@]}"; do
      if kill -0 -- "-$pid" 2>/dev/null; then active=1; fi
    done
    ((active)) || break
    sleep 0.2
  done
  for pid in "${PIDS[@]}"; do kill -KILL -- "-$pid" 2>/dev/null || true; done
  for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
start() {
  local name="$1"
  shift
  setsid "$@" >>"$LOG_DIR/$name.log" 2>&1 &
  PIDS+=("$!")
  NAMES+=("$name")
  printf '已启动 %s；日志：%s/%s.log\n' "$name" "$LOG_DIR" "$name"
}
start api "$BINARY" serve --bind "127.0.0.1:$API_PORT"
start web pnpm --dir "$REPO/web" exec astro dev --host 127.0.0.1 --port "$WEB_PORT" --ignore-lock
printf '浏览器：http://127.0.0.1:%s\n配置：%s\n' "$WEB_PORT" "$SLORAY_CONFIG_FILE"
printf '等待 API 就绪后启动 worker；首次请先启动数据库并在 /setup 完成初始化。\n'
printf 'Ctrl+C 停止本次 API、前端与 worker；数据库保留运行。\n'

worker_started=0
while :; do
  for i in "${!PIDS[@]}"; do
    if ! kill -0 "${PIDS[$i]}" 2>/dev/null; then
      status=0
      wait "${PIDS[$i]}" || status=$?
      die "${NAMES[$i]} 意外退出（状态 $status）；检查 $LOG_DIR/${NAMES[$i]}.log，已停止本次其它进程"
    fi
  done
  if ((worker_started == 0)) && curl --noproxy '*' --fail --silent --output /dev/null \
    --connect-timeout 1 --max-time 2 "$SLORAY_API_INTERNAL_URLS/readyz"; then
    start worker "$BINARY" worker
    worker_started=1
    printf 'worker 已持续运行，负责媒体派生、定时发布及 outbox；修改存储设置后请重启本脚本。\n'
  fi
  sleep 1
done
