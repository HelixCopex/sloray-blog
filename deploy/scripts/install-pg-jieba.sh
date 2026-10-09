#!/usr/bin/env bash
# 可选安装 pg_jieba；构建失败时应用自动使用 jieba-rs 兜底。
set -euo pipefail
PG_MAJOR="${PG_MAJOR:-17}"
DB_NAME="${DB_NAME:-sloray}"
BUILD_DIR="${BUILD_DIR:-/usr/local/src}"
PG_CONFIG="${PG_CONFIG:-/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config}"
[[ ${EUID} -eq 0 ]] || { echo '请用 root 执行' >&2; exit 1; }
[[ -x "$PG_CONFIG" ]] || { echo '缺少目标版本的 pg_config/server-dev' >&2; exit 1; }
command -v cmake >/dev/null || { echo '请安装 cmake' >&2; exit 1; }
install -d "$BUILD_DIR"
source_dir="$BUILD_DIR/pg_jieba"
if [[ ! -d "$source_dir" ]]; then
  git clone --depth 1 --recurse-submodules https://github.com/jaiminpan/pg_jieba.git "$source_dir"
fi
git -C "$source_dir" submodule update --init --recursive
# 上游是 CMake 工程；PG_CONFIG 选择头文件、库与安装目录，避免装错 PG 大版本。
if ! cmake -S "$source_dir" -B "$source_dir/build" \
      -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
      -DPostgreSQL_PG_CONFIG="$PG_CONFIG" \
      -DPostgreSQL_TYPE_INCLUDE_DIR="$("$PG_CONFIG" --includedir-server)" \
      -DPostgreSQL_INCLUDE_DIR="$("$PG_CONFIG" --includedir)" \
   || ! cmake --build "$source_dir/build" -j"$(nproc)"; then
  echo 'pg_jieba 构建失败；可选 zhparser + SCWS，或直接使用自动 jieba-rs 兜底。' >&2
  exit 1
fi
cmake --install "$source_dir/build"
sudo -u postgres psql -v ON_ERROR_STOP=1 --dbname="$DB_NAME" \
  -c 'CREATE EXTENSION IF NOT EXISTS pg_jieba'
result="$(sudo -u postgres psql -v ON_ERROR_STOP=1 --dbname="$DB_NAME" -tAc \
  "SELECT to_tsvector('public.jiebacfg','分布式博客架构') @@ websearch_to_tsquery('public.jiebacfg','博客')")"
[[ "$result" == t ]] || { echo '扩展已安装但中文分词验证失败' >&2; exit 1; }
echo '中文子词命中已验证。不要修改 default_text_search_config。'
echo '首次迁移自动探测；已有数据库切换步骤见 docs/search.md。'
