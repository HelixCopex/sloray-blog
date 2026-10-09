#!/usr/bin/env bash
# 验证失败不能触发旧备份清理；真实恢复另见 smoke-backup.py。
set -euo pipefail
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$WORK/bin" "$WORK/backups"
cat > "$WORK/bin/pg_dump" <<'MOCK'
#!/usr/bin/env bash
[[ "$*" != *password* ]] || exit 99
for arg in "$@"; do
  if [[ "$arg" == --file=* ]]; then printf 'backup' > "${arg#--file=}"; fi
done
MOCK
cat > "$WORK/bin/pg_restore" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
cat > "$WORK/bin/rclone" <<'MOCK'
#!/usr/bin/env bash
exit "${UPLOAD_FAIL:-0}"
MOCK
chmod +x "$WORK/bin/"*
export PATH="$WORK/bin:$PATH" SLORAY_DATABASE_URL='postgres://user:password@localhost/test'
export BACKUP_DIR="$WORK/backups" SLORAY_BACKUP_BUCKET='remote:backup' SLORAY_BACKUP_UPLOADER=rclone
printf 'old' > "$BACKUP_DIR/sloray-old.dump"
touch -d '40 days ago' "$BACKUP_DIR/sloray-old.dump"
if UPLOAD_FAIL=1 "$SCRIPT_DIR/backup-postgres.sh" >/dev/null 2>&1; then echo 'FAIL 上传错误被忽略'; exit 1; fi
[[ -f "$BACKUP_DIR/sloray-old.dump" ]] || { echo 'FAIL 失败后删除了旧备份'; exit 1; }
echo 'PASS 上传失败保留旧备份；数据库密码未进入 pg_dump 参数'
"$SCRIPT_DIR/backup-postgres.sh" >/dev/null
[[ ! -f "$BACKUP_DIR/sloray-old.dump" ]] || { echo 'FAIL 成功后未清理旧备份'; exit 1; }
if RETAIN_DAYS=0 "$SCRIPT_DIR/backup-postgres.sh" >/dev/null 2>&1; then echo 'FAIL 接受非法保留期'; exit 1; fi
echo 'PASS 成功后清理旧备份；非法保留期失败关闭'
