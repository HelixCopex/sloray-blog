#!/usr/bin/env bash
# 完整发布目录 → 原子切换；失败恢复 current 和 systemd 单元。
# 代理配置与 TLS 由主机运维维护，应用发布不覆盖 Nginx 配置。
set -Eeuo pipefail

SRC="${1:?用法: activate-release.sh <发布目录>}"
ROOT="${SLORAY_ROOT:-/srv/sloray}"
KEEP_RELEASES="${KEEP_RELEASES:-5}"
LAYOUT_ONLY="${SLORAY_LAYOUT_ONLY:-0}"
UNIT_DIR="${SLORAY_UNIT_DIR:-/etc/systemd/system}"
SHARED_DIR="${SLORAY_SHARED:-$ROOT/shared}"
API_INSTANCES=(sloray@8080 sloray@8081)
UNIT_NAMES=(sloray@.service sloray-web.service sloray-worker.service sloray-worker.timer)
log() { printf '[activate] %s\n' "$*"; }
die() { printf '[activate] 错误: %s\n' "$*" >&2; return 1; }

[[ -d "$SRC" && -f "$SRC/VERSION" ]] || die "发布目录缺少 VERSION"
[[ -x "$SRC/bin/sloray" ]] || die "发布目录缺少可执行的 bin/sloray"
[[ -f "$SRC/web/dist/server/entry.mjs" ]] || die "发布目录缺少 Astro SSR 入口"
[[ -x "$SRC/deploy/scripts/rollout.sh" ]] || die "发布目录缺少 rollout.sh"
[[ "$KEEP_RELEASES" =~ ^[1-9][0-9]*$ ]] || die "KEEP_RELEASES 必须是正整数"
VERSION="$(cat "$SRC/VERSION")"
[[ "$VERSION" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$VERSION" != *..* ]] \
  || die "非法 VERSION：$VERSION"
ROOT="$(readlink -m "$ROOT")"
[[ "$ROOT" != / ]] || die "发布根目录不能是 /"
for path in "$ROOT" "$UNIT_DIR" "$SHARED_DIR"; do
  [[ "$path" =~ ^/[A-Za-z0-9_./-]+$ ]] || die "部署路径只能包含字母、数字、斜杠、点、下划线和连字符"
done
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
python3 "$SCRIPT_DIR/validate-release.py" --directory "$SRC"
RELEASES_DIR="$ROOT/releases"
DEST="$RELEASES_DIR/$VERSION"
install -d -m 0755 "$RELEASES_DIR"
# 同一主机只允许一个发布者，防止互相切换 symlink 与清理版本。
exec 9>"$ROOT/.deploy.lock"
flock -n 9 || die "另一个发布正在进行"
[[ ! -e "$DEST" && ! -L "$DEST" ]] || die "版本已存在，拒绝覆盖：$VERSION"
PREV="$(readlink -f "$ROOT/current" 2>/dev/null || true)"
if [[ -f "$SRC/SHA256SUMS" ]]; then
  (cd "$SRC" && sha256sum -c --quiet SHA256SUMS) || die "发布目录校验失败"
fi

# 在触碰 current 或单元前保存回滚材料。
SNAPSHOT="$(mktemp -d)"
trap 'rm -rf "$SNAPSHOT"' EXIT
if [[ "$LAYOUT_ONLY" != 1 ]]; then
  for name in "${UNIT_NAMES[@]}"; do
    [[ ! -L "$UNIT_DIR/$name" ]] || die "单元目标不能是符号链接：$name"
    if [[ -f "$UNIT_DIR/$name" ]]; then cp -a "$UNIT_DIR/$name" "$SNAPSHOT/$name"; fi
  done
fi
changed_current=0
changed_units=0
rollback() {
  local rc=$?
  trap - ERR INT TERM
  set +e
  log "发布失败，恢复上一版本与 systemd 单元"
  if (( changed_current )); then
    if [[ -n "$PREV" && -d "$PREV" ]]; then
      ln -sfn "$PREV" "$ROOT/.current.new"
      mv -Tf "$ROOT/.current.new" "$ROOT/current"
    else
      rm -f "$ROOT/current"
    fi
  fi
  if [[ "$LAYOUT_ONLY" != 1 ]]; then
    if (( changed_units )); then
      for name in "${UNIT_NAMES[@]}"; do
        if [[ -f "$SNAPSHOT/$name" ]]; then
          cp -a "$SNAPSHOT/$name" "$UNIT_DIR/$name"
        else
          rm -f "$UNIT_DIR/$name"
        fi
      done
      systemctl daemon-reload
    fi
    if [[ -n "$PREV" && -d "$PREV" ]]; then
      systemctl restart "${API_INSTANCES[@]}" sloray-web
      if [[ -f "$UNIT_DIR/sloray-worker.timer" ]]; then systemctl start sloray-worker.timer; fi
    else
      systemctl stop "${API_INSTANCES[@]}" sloray-web sloray-worker.timer
    fi
  fi
  (( rc != 0 )) || rc=1
  exit "$rc"
}
trap rollback ERR INT TERM

log "安装 $VERSION → $DEST"
cp -a "$SRC" "$DEST"
if [[ ${EUID} -eq 0 ]]; then chown -R root:root "$DEST"; fi

if [[ "$LAYOUT_ONLY" != 1 ]]; then
  install -d -m 0755 "$UNIT_DIR"
  # 配置目录必须在服务启动前存在，且仅服务用户可写。
  install -d -m 0700 "$SHARED_DIR"
  if [[ ${EUID} -eq 0 ]]; then chown sloray:sloray "$SHARED_DIR"; fi
  for name in "${UNIT_NAMES[@]}"; do
    tpl="$DEST/deploy/systemd/$name.in"
    [[ -f "$tpl" ]] || die "缺少单元模板 $name"
    # 使用稳定的 current 路径：回滚 symlink 后服务必定启动旧二进制。
    sed -e "s|@SLORAY_BIN@|$ROOT/current/bin/sloray|g" \
        -e "s|@SLORAY_SHARED@|$SHARED_DIR|g" \
        -e "s|@WEB_ENTRY@|$ROOT/current/web/dist/server/entry.mjs|g" \
        -e "s|@WEB_WORKDIR@|$ROOT/current/web/dist/server|g" \
        "$tpl" > "$SNAPSHOT/new-$name"
    if ! cmp -s "$SNAPSHOT/new-$name" "$UNIT_DIR/$name"; then
      changed_units=1
      install -m 0644 "$SNAPSHOT/new-$name" "$UNIT_DIR/$name"
    fi
  done
  if (( changed_units )); then systemctl daemon-reload; fi
  systemctl enable "${API_INSTANCES[@]}" sloray-web sloray-worker.timer
fi

ln -sfn "$DEST" "$ROOT/.current.new"
# 设置标志先于 mv：即使 mv 被信号打断也能恢复到已知状态。
changed_current=1
mv -Tf "$ROOT/.current.new" "$ROOT/current"
log "current → $VERSION"
if [[ "$LAYOUT_ONLY" != 1 ]]; then "$ROOT/current/deploy/scripts/rollout.sh"; fi
trap - ERR INT TERM

# 保留当前与上一版本；清理失败不影响已成功上线的服务。
kept=0
while IFS= read -r old; do
  [[ -n "$old" ]] || continue
  kept=$((kept + 1))
  [[ "$old" == "$DEST" || "$old" == "$PREV" ]] && continue
  if (( kept > KEEP_RELEASES )); then
    log "清理旧版本 $(basename "$old")"
    rm -rf -- "$old" || log "无法清理 $old"
  fi
done < <(find "$RELEASES_DIR" -mindepth 1 -maxdepth 1 -type d -printf '%T@ %p\n' \
          | sort -rn | cut -d' ' -f2-)
log "$VERSION 上线完成"
