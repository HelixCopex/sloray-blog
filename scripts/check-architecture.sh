#!/usr/bin/env bash
#
# 架构与合规约束检查。
#
#   ./scripts/check-architecture.sh
#
# 与 CI 的 `governance` job 跑的是同一个脚本 —— 本地过不了，CI 一定过不了。
# 这样"硬性约束"才是真的硬性，而不是文档里的一句口号。
set -euo pipefail

cd "$(dirname "$0")/.."

MAX_LINES="${MAX_LINES:-500}"
fail=0

# ---------------------------------------------------------------- 1. 行数上限
echo "== 单文件行数上限：${MAX_LINES} 行（*.rs 与 *.sh） =="

violations=0
files="$(mktemp)"
trap 'rm -f "$files"' EXIT
rg --files --hidden -g '*.rs' -g '*.sh' -g '!target/**' -g '!**/node_modules/**' -g '!.git/**' -g '!.dev-cache/**' > "$files"
sort -o "$files" "$files"
while IFS= read -r f; do
  n=$(wc -l < "$f")
  if (( n > MAX_LINES )); then
    printf '  ✗ %5d 行  %s\n' "$n" "$f"
    violations=$((violations + 1))
  fi
done < "$files"

if (( violations > 0 )); then
  echo
  echo "  有 ${violations} 个文件超过 ${MAX_LINES} 行。"
  echo "  请按职责拆分（例如把「页面渲染」与「表单处理」分开），"
  echo "  而不是靠压缩格式或删注释来凑数。见 AGENTS.md 第 1.1 节。"
  fail=1
else
  echo "  ✓ 全部合规"
fi

# ---------------------------------------------------------------- 2. 依赖门禁
echo
echo "== 依赖：许可证 / 安全公告 / 来源 / 禁用列表 =="
if command -v cargo-deny >/dev/null 2>&1; then
  # --log-level error：只看阻断项。
  # duplicate 之类的 warning 会打印整棵依赖树，把输出淹掉；
  # 想看细节请直接跑 `cargo deny check`。
  if cargo deny --workspace --all-features --locked --log-level error check advisories bans licenses sources; then
    echo "  ✓ advisories / bans / licenses / sources 全部通过"
  else
    echo "  依赖检查未通过。修法见 AGENTS.md 第 4 节。"
    fail=1
  fi
else
  echo "  ! 未安装 cargo-deny，跳过。安装：cargo install cargo-deny --locked"
  fail=1
fi

# ---------------------------------------------------------------- 3. crate 卫生
echo
echo "== 内部 crate 的 version 必须与 workspace 版本一致 =="
if ! python3 scripts/check-crates.py; then
  fail=1
fi

echo
if (( fail )); then
  echo "结果：有未通过项"
  exit 1
fi
echo "结果：全部通过"
