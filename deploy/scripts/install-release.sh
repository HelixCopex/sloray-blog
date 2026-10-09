#!/usr/bin/env bash
#
# 从发布包（tarball）安装。适用于离线环境或 CI 预构建二进制的场景。
#
# 主路径其实是 deploy-on-vps.sh（在服务器上编译）—— 见 docs/deployment.md。
# 这个脚本保留下来是因为它更适合：只能走离线介质、或不想在 VPS 上装 Rust 工具链的场合。
#
# 真正的"安装"逻辑在 activate-release.sh 里，这里只负责校验与解包，
# 避免同一套切换/回滚逻辑被实现两遍。
#
# 用法：sudo install-release.sh /tmp/sloray-v0.1.0.tar.gz
#
# ⚠️ 关于回滚的限制：本脚本能回滚**二进制**，但**不回滚数据库迁移**。
#    所以迁移必须写成向后兼容的（expand / contract 两阶段）：
#    先加列（新代码写、旧代码忽略），确认稳定后再在后续版本里删旧列。
set -euo pipefail

ROOT="${SLORAY_ROOT:-/srv/sloray}"
TARBALL="${1:?用法: sudo install-release.sh <sloray-<version>.tar.gz>}"

log() { printf '[release] %s\n' "$*"; }
die() { printf '[release] 错误: %s\n' "$*" >&2; exit 1; }

[[ ${EUID} -eq 0 ]] || die "需要 root：要写 ${ROOT} 并重启服务"
[[ -f "$TARBALL" ]] || die "找不到发布包：${TARBALL}"
TARBALL="$(readlink -f "$TARBALL")"

# ---- 校验完整性 ----
if [[ -f "${TARBALL}.sha256" ]]; then
  ( cd "$(dirname "$TARBALL")" && sha256sum -c --quiet "$(basename "${TARBALL}.sha256")" ) \
    || die "sha256 校验失败，拒绝安装"
  log "发布包校验和通过"
else
  log "警告：没有 .sha256 文件，跳过整体校验"
fi

# ---- 解包前检查，避免 root 解包越界路径或链接 ----
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
python3 "$SCRIPT_DIR/validate-release.py" --archive "$TARBALL"
# ---- 解包 ----
STAGE="$(mktemp -d)"
trap 'rm -rf "${STAGE}"' EXIT
tar --no-same-owner --no-same-permissions -xzf "$TARBALL" -C "$STAGE"

SRC="$(find "$STAGE" -mindepth 1 -maxdepth 1 -type d)"
[[ -n "$SRC" && -f "$SRC/VERSION" ]] || die "发布包结构不对：缺少 VERSION 文件"

python3 "$SCRIPT_DIR/validate-release.py" --directory "$SRC"
if [[ -f "$SRC/SHA256SUMS" ]]; then
  ( cd "$SRC" && sha256sum -c --quiet SHA256SUMS ) || die "包内文件校验失败"
fi

# ---- 交给共享的安装逻辑 ----
# 这里**不能**用 exec：exec 会替换掉当前进程，trap 里的临时目录清理就不会执行了。
"${SRC}/deploy/scripts/activate-release.sh" "$SRC"
