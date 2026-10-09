#!/usr/bin/env bash
#
# 在沙箱里真跑一遍 activate-release.sh：验证原子切换、版本保留与**回滚**。
#
# 这些逻辑只会在发布失败时才生效 —— 那是你最不希望它第一次被执行的时刻。
# 所以用假的 systemctl 与假的 rollout.sh 把它跑通。
#
# 用法：deploy/scripts/test-activate-release.sh
set -euo pipefail

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

ROOT="$WORK/srv/sloray"
UNIT_DIR="$WORK/etc/systemd/system"
PROXY_DIR="$WORK/etc/nginx"
mkdir -p "$ROOT/releases" "$UNIT_DIR" "$PROXY_DIR"
: > "$PROXY_DIR/sloray.conf"

# 伪造 systemctl：记录调用但永远成功（沙箱里没有 systemd）。
BIN="$WORK/bin"; mkdir -p "$BIN"
cat > "$BIN/systemctl" <<'SH'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >> "${SYSTEMCTL_LOG:-/dev/null}"
exit 0
SH
chmod +x "$BIN/systemctl"
export PATH="$BIN:$PATH"
export SYSTEMCTL_LOG="$WORK/systemctl.log"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTIVATE="$SCRIPT_DIR/activate-release.sh"

FAILED=0
check() { # check <描述> <期望> <实际>
  if [[ "$2" == "$3" ]]; then
    printf '  PASS  %s\n' "$1"
  else
    printf '  FAIL  %s\n        期望: %s\n        实际: %s\n' "$1" "$2" "$3"
    FAILED=1
  fi
}

# 组装一个假的发布目录。$2 是 rollout.sh 的退出码 —— 用来注入发布失败。
make_release() {
  # 注意：`local` 会在赋值之前先把所有参数展开，
  # 所以 d=... 里的 $name 必须放到另一条 local 语句里，否则 set -u 会报未绑定。
  local name="$1" rc="$2" ver="${3:-$1}"
  local d="$WORK/stage-$name"
  rm -rf "$d"
  mkdir -p "$d/bin" "$d/web/dist/server" "$d/deploy/systemd" "$d/deploy/scripts"
  printf '#!/bin/sh\nexit 0\n' > "$d/bin/sloray"
  chmod +x "$d/bin/sloray"
  printf 'export {};\n' > "$d/web/dist/server/entry.mjs"
  # 单元现在是**模板**（.service.in），由 activate-release.sh 渲染。
  printf '[Unit]\nDescription=fake\nExecStart=@SLORAY_BIN@ serve\nReadWritePaths=@SLORAY_SHARED@\n' \
    > "$d/deploy/systemd/sloray@.service.in"
  printf '[Unit]\nDescription=fakeweb\nExecStart=/usr/bin/node @WEB_ENTRY@\nWorkingDirectory=@WEB_WORKDIR@\n' \
    > "$d/deploy/systemd/sloray-web.service.in"
  cp "$SCRIPT_DIR/../systemd/sloray-worker.service.in" "$d/deploy/systemd/"
  cp "$SCRIPT_DIR/../systemd/sloray-worker.timer.in" "$d/deploy/systemd/"
  printf '#!/bin/sh\nexit %s\n' "$rc" > "$d/deploy/scripts/rollout.sh"
  chmod +x "$d/deploy/scripts/rollout.sh"
  printf '%s\n' "$ver" > "$d/VERSION"
  printf '%s\n' "$d"
}

run_activate() {
  SLORAY_ROOT="$ROOT" \
  SLORAY_UNIT_DIR="$UNIT_DIR" \
  KEEP_RELEASES=5 \
    "$ACTIVATE" "$1"
}

echo "== 1) 首次安装 v1 =="
release1="$(make_release v1 0)"
if run_activate "$release1" > "$WORK/out1.log" 2>&1; then
  printf '  PASS  安装返回 0\n'
else
  printf '  FAIL  首次安装失败\n'; sed 's/^/        /' "$WORK/out1.log"; FAILED=1
fi
check "current 指向 v1"  "$ROOT/releases/v1" "$(readlink -f "$ROOT/current")"
check "VERSION 内容正确" "v1"               "$(cat "$ROOT/current/VERSION")"
check "二进制可执行位保留" "yes"            "$([[ -x "$ROOT/current/bin/sloray" ]] && echo yes || echo no)"
check "systemd 单元已渲染安装" "yes" "$([[ -f "$UNIT_DIR/sloray@.service" ]] && echo yes || echo no)"
check "占位符已全部替换"        "yes" "$(grep -q '@SLORAY_BIN@\|@SLORAY_SHARED@' "$UNIT_DIR/sloray@.service" 2>/dev/null && echo no || echo yes)"
check "单元指向稳定 current 二进制" "yes" "$(grep -q "$ROOT/current/bin/sloray" "$UNIT_DIR/sloray@.service" 2>/dev/null && echo yes || echo no)"
check "Web 单元也渲染了"        "yes" "$(grep -q '@WEB_ENTRY@' "$UNIT_DIR/sloray-web.service" 2>/dev/null && echo no || echo yes)"
check "调用了 systemctl"   "yes"            "$([[ -s "$SYSTEMCTL_LOG" ]] && echo yes || echo no)"

echo
echo "== 2) 升级到 v2；v1 应保留、单元未变化时不应重复 reload =="
rm -f "$SYSTEMCTL_LOG"
release2="$(make_release v2 0)"
if run_activate "$release2" > "$WORK/out2.log" 2>&1; then
  printf '  PASS  安装返回 0（说明 units 未变化时不会误触 systemd）\n'
else
  printf '  FAIL  第二次安装失败（很可能是 set -e 与 (( )) 的交互问题）\n'
  sed 's/^/        /' "$WORK/out2.log"; FAILED=1
fi
check "current 指向 v2"      "$ROOT/releases/v2" "$(readlink -f "$ROOT/current")"
check "v1 仍在 releases 里"  "yes"              "$([[ -d "$ROOT/releases/v1" ]] && echo yes || echo no)"

echo
echo "== 3) v3 的 rollout 失败 → 必须自动回滚到 v2 =="
release3="$(make_release v3 1)"
if run_activate "$release3" > "$WORK/out3.log" 2>&1; then
  printf '  FAIL  失败的发布竟然返回了 0\n'; FAILED=1
else
  printf '  PASS  失败的发布返回非 0\n'
fi
check "current 已回滚到 v2"   "$ROOT/releases/v2" "$(readlink -f "$ROOT/current")"
check "v2 的产物完好"         "v2"               "$(cat "$ROOT/current/VERSION")"
check "v3 目录保留以便排查"   "yes"              "$([[ -d "$ROOT/releases/v3" ]] && echo yes || echo no)"
check "回滚时重启了服务"      "yes"              "$(grep -q 'systemctl restart' "$SYSTEMCTL_LOG" && echo yes || echo no)"

echo
echo "== 4) 路径穿越防护：VERSION 里的 ../ 不能逃出 releases =="
# 这是真实威胁：VERSION 来自发布包/构建产物，而 DEST 会被 rm -rf。
# 只在字符串上做前缀比较是不够的 —— releases/../../x 确实以 releases/ 开头。
evil="$(make_release evil 0 '../../pwned')"
if run_activate "$evil" >/dev/null 2>&1; then
  printf '  FAIL  竟然接受了含 ../ 的 VERSION\n'; FAILED=1
else
  printf '  PASS  含 ../ 的 VERSION 被拒绝\n'
fi
check "releases 之外没有出现任何东西" "no" \
  "$([[ -e "$WORK/pwned" ]] && echo yes || echo no)"

echo
echo "== 5) 绝对路径 VERSION 同样要被拒绝 =="
evil2="$(make_release evil2 0 '/tmp/pwned-abs')"
if run_activate "$evil2" >/dev/null 2>&1; then
  printf '  FAIL  竟然接受了绝对路径 VERSION\n'; FAILED=1
else
  printf '  PASS  绝对路径 VERSION 被拒绝\n'
fi

echo
check "回滚后单元仍通过 current 启动" "yes" "$(grep -q "$ROOT/current/bin/sloray" "$UNIT_DIR/sloray@.service" && echo yes || echo no)"
echo "== 6) 已存在的版本必须拒绝，不能删掉正在运行的产物 =="
if run_activate "$release2" >/dev/null 2>&1; then
  printf '  FAIL  重复版本被覆盖\n'; FAILED=1
else
  printf '  PASS  重复版本被拒绝\n'
fi
check "当前版本仍完好" "v2" "$(cat "$ROOT/current/VERSION")"

echo
if (( FAILED )); then
  echo "结果：有失败项"
  exit 1
fi
echo "结果：全部通过"
