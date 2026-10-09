#!/usr/bin/env bash
# 逻辑备份；PITR 另需物理基础备份与连续 WAL 归档，见 docs/backup.md。
set -euo pipefail
umask 077
BACKUP_DIR="${BACKUP_DIR:-/var/backups/sloray}"
RETAIN_DAYS="${RETAIN_DAYS:-14}"
S3_BUCKET="${SLORAY_BACKUP_BUCKET:-}"
: "${SLORAY_DATABASE_URL:?必须设置 SLORAY_DATABASE_URL}"
[[ "$RETAIN_DAYS" =~ ^[1-9][0-9]*$ ]] || { echo 'RETAIN_DAYS 必须是正整数' >&2; exit 1; }
[[ ! -L "$BACKUP_DIR" && "$(readlink -m "$BACKUP_DIR")" != / ]] || { echo '备份目录无效' >&2; exit 1; }
for tool in pg_dump pg_restore python3 sha256sum flock; do
  command -v "$tool" >/dev/null || { echo "缺少 $tool" >&2; exit 1; }
done
uploader="${SLORAY_BACKUP_UPLOADER:-}"
if [[ -n "$S3_BUCKET" ]]; then
  if [[ -z "$uploader" ]]; then
    if command -v rclone >/dev/null; then uploader=rclone; else uploader=aws; fi
  fi
  [[ "$uploader" == rclone || "$uploader" == aws ]] || { echo '上传器必须是 rclone 或 aws' >&2; exit 1; }
  command -v "$uploader" >/dev/null || { echo '已配置远端但缺少上传器，中止备份清理' >&2; exit 1; }
fi
install -d -m 0700 "$BACKUP_DIR"
exec 9>"$BACKUP_DIR/.backup.lock"
flock -n 9 || { echo '备份正在运行' >&2; exit 1; }
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
partial="$(mktemp "$BACKUP_DIR/.sloray-${stamp}-XXXXXX.partial")"
trap 'rm -f "$partial"' EXIT
outfile="$BACKUP_DIR/sloray-${stamp}-$(basename "$partial" | cut -d- -f3 | cut -d. -f1).dump"
# 密码只进入子进程环境，不放入命令参数或日志；支持 URI 的 libpq 常用参数。
python3 - "$partial" <<'PY'
import os, subprocess, sys, urllib.parse
parts = urllib.parse.urlsplit(os.environ['SLORAY_DATABASE_URL'])
if parts.scheme not in ('postgres', 'postgresql'):
    raise SystemExit('数据库连接串无效')
env = dict(os.environ)
values = {'PGHOST': parts.hostname, 'PGPORT': str(parts.port) if parts.port else None,
          'PGUSER': urllib.parse.unquote(parts.username) if parts.username else None,
          'PGPASSWORD': urllib.parse.unquote(parts.password) if parts.password else None,
          'PGDATABASE': urllib.parse.unquote(parts.path.removeprefix('/'))}
options = {'sslmode':'PGSSLMODE', 'sslrootcert':'PGSSLROOTCERT', 'sslcert':'PGSSLCERT',
           'sslkey':'PGSSLKEY', 'host':'PGHOST', 'port':'PGPORT', 'user':'PGUSER',
           'connect_timeout':'PGCONNECT_TIMEOUT', 'options':'PGOPTIONS'}
for key, value in urllib.parse.parse_qsl(parts.query):
    if key not in options: raise SystemExit('连接串含备份工具未支持的参数：'+key)
    values[options[key]] = value
for key, value in values.items():
    if value is not None: env[key] = value
result = subprocess.run(['pg_dump', '--format=custom', '--no-owner', '--no-privileges', '--file='+sys.argv[1]], env=env)
sys.exit(result.returncode)
PY
pg_restore --list "$partial" >/dev/null
mv "$partial" "$outfile"
(cd "$BACKUP_DIR" && sha256sum "$(basename "$outfile")" > "$(basename "$outfile").sha256")
if [[ -n "$S3_BUCKET" ]]; then
  if [[ "$uploader" == rclone ]]; then
    rclone copy "$outfile" "$S3_BUCKET"
    rclone copy "$outfile.sha256" "$S3_BUCKET"
  else
    aws s3 cp "$outfile" "${S3_BUCKET%/}/"
    aws s3 cp "$outfile.sha256" "${S3_BUCKET%/}/"
  fi
fi
# 只有完整备份与配置的上传均成功后，才清理旧产物。
find "$BACKUP_DIR" -type f \( -name 'sloray-*.dump' -o -name 'sloray-*.dump.sha256' \) -mtime "+$RETAIN_DAYS" -delete
printf '[backup] 完成：%s（目录可读并已生成 SHA-256；仍需恢复演练）\n' "$outfile"
