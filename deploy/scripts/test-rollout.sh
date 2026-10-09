#!/usr/bin/env bash
# 不接触真实 systemd；验证生产 rollout 的控制流和就绪等待。
set -euo pipefail
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin" "$WORK/root/current/bin" "$WORK/root/shared"
export SLORAY_ROOT="$WORK/root" SLORAY_ENV_FILE="$WORK/api.env"
export SLORAY_SHARED="$WORK/root/shared" READY_TIMEOUT_SECS=2
export TEST_LOG="$WORK/calls" TEST_COUNTER="$WORK/counter"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cat > "$WORK/bin/systemctl" <<'SH'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >> "$TEST_LOG"
SH
cat > "$WORK/bin/systemd-run" <<'SH'
#!/usr/bin/env bash
printf 'systemd-run %s\n' "$*" >> "$TEST_LOG"
exit "${MIGRATION_RC:-0}"
SH
cat > "$WORK/bin/sleep" <<'SH'
#!/usr/bin/env bash
exit 0
SH
cat > "$WORK/bin/curl" <<'SH'
#!/usr/bin/env bash
printf 'curl %s\n' "$*" >> "$TEST_LOG"
n=0
if [[ -f "$TEST_COUNTER" ]]; then n=$(cat "$TEST_COUNTER"); fi
n=$((n + 1)); printf '%s' "$n" > "$TEST_COUNTER"
if [[ "${PROBE_ALWAYS_FAIL:-0}" == 1 ]]; then exit 22; fi
if [[ "$n" == 1 ]]; then exit 22; fi
SH
chmod +x "$WORK/bin/"*
export PATH="$WORK/bin:$PATH"

"$SCRIPT_DIR/rollout.sh" > "$WORK/first.log"
grep -q '/healthz' "$TEST_LOG"
if grep -q 'systemd-run' "$TEST_LOG"; then echo 'FAIL 初次启动不应提前迁移'; exit 1; fi
echo 'PASS 首次启动检查 healthz；第一次探测失败后仍等待成功'

printf '{}' > "$SLORAY_SHARED/config.json"
printf 'SLORAY_DATABASE_URL=postgres://test\n' > "$SLORAY_ENV_FILE"
: > "$TEST_LOG"
printf '0' > "$TEST_COUNTER"
"$SCRIPT_DIR/rollout.sh" > "$WORK/configured.log"
grep -q "EnvironmentFile=-$SLORAY_ENV_FILE" "$TEST_LOG"
grep -q '/readyz' "$TEST_LOG"
grep -q 'systemctl restart sloray@8081' "$TEST_LOG"
echo 'PASS 已配置发布沿用 API 环境文件，逐个等待 readyz'

: > "$TEST_LOG"
if MIGRATION_RC=1 "$SCRIPT_DIR/rollout.sh" > "$WORK/migration.log" 2>&1; then
  echo 'FAIL 迁移失败被忽略'; exit 1
fi
if grep -q 'systemctl restart' "$TEST_LOG"; then echo 'FAIL 迁移失败后重启了服务'; exit 1; fi
echo 'PASS 迁移失败在重启前中止'

: > "$TEST_LOG"
if PROBE_ALWAYS_FAIL=1 "$SCRIPT_DIR/rollout.sh" > "$WORK/timeout.log" 2>&1; then
  echo 'FAIL 等待超时被忽略'; exit 1
fi
if grep -q 'systemctl restart sloray@8081' "$TEST_LOG"; then echo 'FAIL 第一实例失败后重启了第二实例'; exit 1; fi
echo 'PASS 第一实例就绪超时后保留第二实例'
echo '结果：全部通过'
