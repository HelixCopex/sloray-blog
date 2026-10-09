#!/usr/bin/env bash
#
# 在 VPS 上编译并发布 —— **Cargo 部署的主路径**。
#
# 为什么在目标机器上编译：
#   * glibc 兼容性问题根本不存在 —— 链接的就是本机 libc，
#     不需要再操心"构建机的 glibc 版本决定运行环境下限"；
#   * 不需要 CI 产出二进制、不需要配置 SSH 部署密钥和 artifact；
#   * `target/` 在多次发布之间复用，第二次起是增量编译，快很多。
#
# 代价：每次发布要吃本机的 CPU 与内存。**实测数据见 docs/deployment.md**，
# 2 vCPU / 2 GB 的机器上必须加 swap 并限制并行度，否则链接阶段会 OOM。
#
# 用法：
#   sudo /srv/sloray/src/deploy/scripts/deploy-on-vps.sh            # 拉取当前分支最新
#   sudo /srv/sloray/src/deploy/scripts/deploy-on-vps.sh v0.2.0     # 切到指定 tag
#
# 环境变量：
#   SLORAY_SRC     源码仓库路径      （默认 /srv/sloray/src）
#   SLORAY_ROOT    发布根目录        （默认 /srv/sloray）
#   CARGO_JOBS     编译并行度        （默认 1，2 GB 内存的机器请保持 1）
#   SLORAY_PUBLIC_SITE_URL  canonical/sitemap 用的域名（构建期注入）
set -euo pipefail

SRC_DIR="${SLORAY_SRC:-/srv/sloray/src}"
ROOT="${SLORAY_ROOT:-/srv/sloray}"
STAGE="${SLORAY_STAGE:-${ROOT}/.stage}"
CARGO_JOBS="${CARGO_JOBS:-1}"
TARGET_REF="${1:-}"
API_INTERNAL_URL="${SLORAY_API_INTERNAL_URL:-http://127.0.0.1:8080,http://127.0.0.1:8081}"

log() { printf '\n[deploy] %s\n' "$*"; }
die() { printf '\n[deploy] 错误: %s\n' "$*" >&2; exit 1; }

[[ ${EUID} -eq 0 ]] || die "需要 root：要切换 releases 并重启服务"
[[ -d "${SRC_DIR}/.git" ]] || die "在 ${SRC_DIR} 找不到 git 仓库，请先 git clone"

# provision-debian.sh 把 Rust 装在 /usr/local/cargo；sudo 会重置 PATH，
# 所以这里显式补上，避免"交互式能跑、脚本里找不到 cargo"这种坑。
if [[ -d /usr/local/cargo/bin ]]; then
  export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
  PATH="/usr/local/cargo/bin:${PATH}"
fi
if [[ -d "${HOME}/.cargo/bin" ]];   then PATH="${HOME}/.cargo/bin:${PATH}"; fi
export PATH

command -v cargo >/dev/null || die "找不到 cargo（见 provision-debian.sh 的构建工具链一节）"
command -v pnpm  >/dev/null || die "找不到 pnpm"

# 规范化后只允许删除发布根下的 staging 目录，拒绝 symlink 和穿越路径。
[[ ! -L "$STAGE" ]] || die "staging 不能是符号链接"
ROOT="$(realpath -m "$ROOT")"
SRC_DIR="$(realpath -m "$SRC_DIR")"
STAGE="$(realpath -m "$STAGE")"
[[ "$ROOT" != / && "$(dirname "$STAGE")" == "$ROOT" ]] || die "staging 必须直接位于发布根目录下"
case "$(basename "$STAGE")" in current|shared|releases|src) die "拒绝删除保留目录";; esac
[[ "$SRC_DIR" != "$STAGE" && "$SRC_DIR" != "$STAGE/"* ]] || die "staging 不能覆盖源码"
[[ "$CARGO_JOBS" =~ ^[1-9][0-9]*$ ]] || die "CARGO_JOBS 必须为正整数"
exec 9>"$ROOT/.build.lock"
flock -n 9 || die "已有构建发布正在运行"

# ---------------------------------------------------------------- 1. 取代码
log "1/5 取代码（${TARGET_REF:-当前分支最新}）"
cd "$SRC_DIR"
[[ -z "$(git status --porcelain)" ]] || die "源码有未提交改动，拒绝覆盖"
git fetch --tags --prune origin
if [[ -n "$TARGET_REF" ]]; then
  git checkout --detach "$(git rev-parse --verify --end-of-options "${TARGET_REF}^{commit}")"
else
  git pull --ff-only
fi
VERSION="$(git describe --tags --always)"
log "版本：${VERSION}"

# ---------------------------------------------------------------- 2. 编译 Rust
log "2/5 编译 Rust（jobs=${CARGO_JOBS}）—— 这一步最慢也最吃内存"
log "     2 GB 内存的机器请保持 CARGO_JOBS=1，并确保有 swap"
CARGO_BUILD_JOBS="$CARGO_JOBS" cargo build --release --locked -p sloray

# ---------------------------------------------------------------- 3. 构建前端
log "3/5 构建前端"
cd "${SRC_DIR}/web"
pnpm install --frozen-lockfile
# ⚠️ 这两个变量必须在这里（构建期）设置：Vite 会把它们内联进产物，
#    运行时再设是无效的。
SLORAY_API_INTERNAL_URL="$API_INTERNAL_URL" \
SLORAY_PUBLIC_SITE_URL="${SLORAY_PUBLIC_SITE_URL:-}" \
  pnpm run build

# ---------------------------------------------------------------- 4. 组装发布目录
log "4/5 组装发布目录"
cd "$SRC_DIR"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/web"

install -m 0755 target/release/sloray "$STAGE/bin/"
# Astro 的 standalone 产物是自包含的（实测没有 node_modules 也能启动），
# 所以只带 dist，不带依赖目录。
cp -r web/dist "$STAGE/web/dist"
rsync -a --exclude=__pycache__ --exclude='*.pyc' deploy/ "$STAGE/deploy/"
cp -r docs    "$STAGE/docs"
cp README.md LICENSE "$STAGE/"
echo "$VERSION" > "$STAGE/VERSION"

# SHA256SUMS 被 find 明确排除，不会读写同一文件。
# shellcheck disable=SC2094
( cd "$STAGE" && find . -type f ! -name SHA256SUMS -print0 \
    | sort -z | xargs -0 sha256sum > SHA256SUMS )

# ---------------------------------------------------------------- 5. 上线
log "5/5 上线（切换 symlink → 迁移 → 滚动重启；失败自动回滚）"
"${SRC_DIR}/deploy/scripts/activate-release.sh" "$STAGE"
