#!/usr/bin/env bash
#
# 滚动发布：先迁移，再逐个重启 API 实例，最后重启 Web。
#
# 为什么 API 侧能逐个重启：
#   * sloray-api 服务无状态 → 新实例起来即可接流量；
#   * 两个实例 + Nginx 被动故障切换 → 重启一个时另一个继续服务；
#   * 迁移独立于应用启动，且用咨询锁保证单飞。
#
# Web 只有单实例，重启有 1–2 秒窗口（Nginx 会返回 502）——这是明确接受的取舍。
# 若将来要 Web 也逐个重启，把它也改成模板单元跑两个实例即可。
#
# 由 install-release.sh 调用；一般不需要手动执行。
set -euo pipefail

ROOT="${SLORAY_ROOT:-/srv/sloray}"
API_INSTANCES=("sloray@8080" "sloray@8081")
READY_TIMEOUT_SECS="${READY_TIMEOUT_SECS:-30}"

log() { printf '[rollout] %s\n' "$*"; }

wait_ready() {
  local port="$1" unit="$2" waited=0
  until curl -fsS "http://127.0.0.1:${port}/${PROBE}" >/dev/null 2>&1; do
    if (( waited >= READY_TIMEOUT_SECS )); then
      log "实例 ${port} 在 ${READY_TIMEOUT_SECS}s 内未就绪，中止发布"
      systemctl status "$unit" --no-pager -l || true
      journalctl -u "$unit" -n 40 --no-pager || true
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  log "实例 ${port} 已就绪"
}

systemctl stop sloray-worker.timer
systemctl stop sloray-worker.service

# 1) 迁移先跑，且只跑一次。
#    失败就直接停 —— 绝不带着「旧代码 + 新 schema」继续跑。
#    注意：已配置的实例也可以直接在 /setup 里由向导跑迁移（同一把咨询锁）。
log "执行数据库迁移"
# systemd-run 使用与 API 相同的 EnvironmentFile，避免 env 与 CLI 配置分叉。
# 文件尚不存在时是首次部署，先启动 setup；向导负责迁移。
SHARED_DIR="${SLORAY_SHARED:-$ROOT/shared}"
ENV_FILE="${SLORAY_ENV_FILE:-/etc/sloray/api.env}"
if [[ -f "$SHARED_DIR/config.json" ]] || { [[ -f "$ENV_FILE" ]] && grep -Eq '^SLORAY_DATABASE_URL=.+' "$ENV_FILE"; }; then
  systemd-run --quiet --wait --pipe --collect \
    --property=User=sloray --property=Group=sloray \
    --property="EnvironmentFile=-$ENV_FILE" \
    --setenv="SLORAY_CONFIG_FILE=$SHARED_DIR/config.json" \
    "$ROOT/current/bin/sloray" migrate up
else
  log "首次部署：迁移由 /setup 向导执行"
fi

# 未初始化时 /readyz 应为 503，只验证进程存活；初始化后必须验证依赖。
PROBE=readyz
if [[ ! -f "$SHARED_DIR/config.json" ]] && ! { [[ -f "$ENV_FILE" ]] && grep -Eq '^SLORAY_DATABASE_URL=.+' "$ENV_FILE"; }; then
  PROBE=healthz
fi

# 2) 逐个重启 API 实例。每次都等它 ready 才动下一个，
#    因此任意时刻至少有一个实例在健康地服务请求。
for unit in "${API_INSTANCES[@]}"; do
  port="${unit##*@}"
  log "重启 ${unit}"
  systemctl restart "$unit"
  wait_ready "$port" "$unit"
done

# 3) 重启 Web（单实例，有秒级窗口）。
log "重启 sloray-web"
systemctl restart sloray-web

# 4) 最后确认一次服务真的起来了。
#    探针虽然也对公网开放，但脚本从本机校验更快，也不受 DNS 与证书影响。
sleep 1
if ! curl -fsS "http://127.0.0.1:8080/${PROBE}" >/dev/null 2>&1; then
  log "发布后 /readyz 校验失败"
  exit 1
fi
if ! curl -fsS -o /dev/null "http://127.0.0.1:4321/about"; then
  log "发布后 Astro 静态页校验失败"
  exit 1
fi

systemctl start sloray-worker.timer
log "滚动发布完成"
