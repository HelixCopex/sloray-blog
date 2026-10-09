#!/usr/bin/env bash
# GitHub Ubuntu runner：与测试服务相同大版本的工具，避免 pg_dump 版本落后。
set -euo pipefail
: "${PG_MAJOR:?必须指定 PostgreSQL 大版本}"
[[ "$PG_MAJOR" =~ ^(16|17|18)$ ]] || exit 1
sudo install -d /usr/share/postgresql-common/pgdg
sudo curl --fail --silent --show-error -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc https://www.postgresql.org/media/keys/ACCC4CF8.asc
# shellcheck source=/dev/null
. /etc/os-release
printf 'deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt %s-pgdg main\n' "$VERSION_CODENAME" | sudo tee /etc/apt/sources.list.d/sloray-pgdg.list >/dev/null
sudo apt-get update -qq
packages=("postgresql-client-$PG_MAJOR")
if [[ "${PG_SERVER:-0}" == 1 ]]; then packages+=("postgresql-$PG_MAJOR"); fi
sudo apt-get install -y "${packages[@]}"
printf '/usr/lib/postgresql/%s/bin\n' "$PG_MAJOR" >> "$GITHUB_PATH"
