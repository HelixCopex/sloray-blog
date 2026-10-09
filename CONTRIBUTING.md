# 贡献指南

本指南适用于人类贡献者与 AI 编码代理。架构、依赖、迁移和验证的硬性约定以
[AGENTS.md](AGENTS.md) 为准；分支策略的决策记录见
[ADR-0019](docs/adr/0019-linear-history-and-dev-releases.md)。

## 开始之前

先阅读 [项目概览](README.md)、[架构](docs/architecture.md) 和
[本地开发指南](docs/development.md)。修改部署、检索或媒体时，再阅读对应专项文档。
新增架构决策须追加 ADR，不能改写已经接受的历史决策。

## 分支与线性历史

| 分支 | 职责 | 更新规则 |
|---|---|---|
| `dev` | 日常开发与集成 | 后续开发更改提交到这里，推送触发完整 CI |
| `main` | 已获项目维护者批准的版本 | 每个版本只从通过 CI 的 `dev` 快进合并一次 |
| 临时工作分支 | 短期隔离改动 | 从 `dev` 建立，整理后以线性历史集成回 `dev` |

**所有分支必须保持线性历史，不允许产生 merge commit。** 合并使用
`git merge --ff-only`；发生分叉时先整理尚未共享的工作提交，不能用普通 merge
或 `--no-ff` 绕过约束。CI 检查 `dev` / `main` 的 push 和 PR 的真实提交历史，PR 的检查对象
是源分支提交，不把 GitHub 自动生成的测试合并提交算作贡献历史。

`main` 不接受日常开发、独立修复、直接版本提交或 cherry-pick。紧急修复也先进入
`dev`，经过同样的 CI 与维护者发布决定。发布时必须保留 `dev` 的原始提交 SHA，
因此 `dev → main` 不能使用 squash merge 或重新生成提交的 rebase merge。

本地克隆后设置快进默认值，避免 pull/merge 意外产生合并提交：

```bash
git config --local pull.ff only
git config --local merge.ff only
git switch dev
git pull --ff-only origin dev
```

每次开始开发前确认当前分支是 `dev`。未另行约定时，AI 代理也必须将本次任务的
更改提交到 `dev`；不能自行推进 `main`、递增版本或创建发布标签。

需要临时工作分支时，从最新 `dev` 创建；完成后先在工作分支上
`git rebase dev`，验证通过，再切回 `dev` 执行 `git merge --ff-only <工作分支>`。
重新整理提交后必须重跑对应验证。不要重写已共享的 `dev` / `main` 历史，
也不要强推它们；共享分支分叉时先协调，不能用强推覆盖他人的提交。

## 提交前的验证

从仓库根目录执行以下门禁，全部通过后提交：

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
./scripts/check-architecture.sh
(
  cd web
  pnpm run api:check
  pnpm run test
  pnpm run check
  pnpm run build
)
git diff --check
```

涉及数据库或部署时额外执行 `deploy/scripts/test-activate-release.sh`，并按变更
范围执行真实数据库、浏览器或恢复验证。完整要求见 [AGENTS.md 第 5 节](AGENTS.md#5-提交与审查)。

提交说明应写清问题、结果与验证证据；报告实际命令输出和环境限制，不能把本地
通过等同于远程 CI 通过。生成契约、锁文件与文档应随对应改动一起提交。

## 版本批准与 main 更新

**版本是否发布以及递增到哪个版本，由项目维护者（仓库所有者）决定。**
CI 通过只是发布的必要条件，不能触发自动版本递增或自动合入 `main`。

每个版本按以下顺序执行：

1. 将本次版本的全部开发改动提交到 `dev`，先确认开发 CI 通过。
2. 由维护者明确决定发布及目标版本号。未经决定，任何人或自动化都不能递增版本。
3. 在 `dev` 提交批准的版本更改：同步 `Cargo.toml` 的 workspace 版本、内部 path
   依赖版本及 `Cargo.lock` 的 workspace 包版本，更新相关发布文档；执行门禁并推送。
4. 等待**包含版本提交的 `dev` 最新 SHA** 的全部 CI 作业成功。失败、取消、仍在运行
   或应运行却未运行的作业都不算通过；PR CI 或旧提交的成功不能代替这次 push CI。
5. 记录维护者批准的目标版本号、提交 SHA 和对应成功的 CI run。此后 `dev` 若新增
   提交，必须重新验证并由维护者确认新的发布 SHA。
6. 将 `main` **一次快进**到该 SHA。若无法快进，停止发布并协调历史，不能改用
   squash、rebase merge、merge commit 或强推。一个版本不能分批多次推进 `main`。
7. 在 `main` 的该提交上创建批准版本对应的 `v<版本号>` 标签，再按
   [部署手册](docs/deployment.md) 发布该标签；开发继续在 `dev`。

以下命令只在上述批准和 CI 确认完成后执行。`APPROVED_SHA` 与 `APPROVED_VERSION`
必须替换为已记录的真实值，不能把示例当作发布授权：

```bash
(
  set -e
  git fetch origin
  APPROVED_SHA='<维护者批准且 dev push CI 全部成功的完整 SHA>'
  APPROVED_VERSION='<维护者批准的递增版本号，不带 v>'
  test "$(git rev-parse origin/dev)" = "$APPROVED_SHA"
  test -z "$(git status --porcelain)"
  git switch main
  git merge --ff-only origin/main
  test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"
  git merge --ff-only "$APPROVED_SHA"
  test "$(git rev-parse HEAD)" = "$APPROVED_SHA"
  git tag -a "v$APPROVED_VERSION" -m "Release v$APPROVED_VERSION"
  git push --atomic origin main "refs/tags/v$APPROVED_VERSION"
  git switch dev
)
```

执行前确认本地 `main` 与 `origin/main` 一致；若本地有独立提交，先协调。
推送若因并发更新被拒绝，重新读取远程状态与 CI 证据，不强推。

## GitHub 配置边界

仓库内的 CI 与文档不能替代 GitHub 的分支保护。维护者配置仓库规则时，应对
`dev` / `main` 要求线性历史、禁止强推与删除；将完整 CI 检查设为必需，并限制
`main` 的更新权限仅供维护者发布。临时分支同样受线性历史约束。

GitHub 上的普通 merge 按钮会生成合并提交；发布应使用上面的快进流程。
若保护规则只允许平台 PR 合并、阻止维护者快进推送，应先调整为允许受控发布，
不能通过 squash/rebase merge 改写已经通过 CI 的发布 SHA。
这些托管平台设置需在 GitHub 实际配置后才能算生效；本指南不表示它们已启用。
