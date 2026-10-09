# AGENTS.md —— 贡献与治理指南

> 本文件是**硬性约定**，不是建议。
> 适用于人类贡献者与 AI 编码代理；与其它文档冲突时以本文件为准。
>
> 项目的"为什么这么设计"在 [`docs/architecture.md`](docs/architecture.md)，
> 逐条决策在 [`docs/adr/`](docs/adr/README.md)。本文件只讲**必须怎么做**。

## 0. 先读什么

| 你要做的事 | 先读 |
|---|---|
| 改任何代码 | 本文件第 1、2 节 |
| 加依赖 | 本文件第 4 节 |
| 改数据库 schema | [`docs/architecture.md`](docs/architecture.md) + 第 5.2 节的迁移纪律 |
| 改部署流程 | [`docs/deployment.md`](docs/deployment.md) |
| 想推翻某个既有决定 | 先写一条新 ADR 取代旧的，不要直接改历史 |

---

## 1. 硬性约束（不满足就无法合入）

### 1.1 单文件不超过 500 行

适用于 `*.rs` 与 `*.sh`。**由 CI 强制**（`scripts/check-architecture.sh`）。

超限时**按职责拆分**，不要靠压缩格式、合并行、删注释来凑数：

* 一个文件里出现两个以上"章节级"注释分隔（`// ---- xxx ----`）时，
  通常就是该拆的信号；
* 典型的拆法是把「渲染」与「处理」分开
  （例：`routes/pages/` 下按页面拆成 `setup.rs` / `login.rs` / `settings.rs`，
  共享的鉴权辅助留在 `mod.rs`）。

> 文档（`*.md`）不受此限制 —— 架构说明与技术手册需要连续叙事，
> 拆成碎文件反而更难读。

### 1.2 按功能分立的 crates

代码必须按职责放在对应 crate 里。**依赖方向是单向的，不许回指**：

```
config  ←  store  ←  auth  ←  setup  ←  web
   ↑                                        ↑
   └──────────── migration ─────────────────┘
                    ↑
              sloray（根二进制，CLI）
```

| crate | 职责 | 允许依赖 |
|---|---|---|
| `sloray-config` | 配置：加载、校验、原子落盘 | 无内部依赖 |
| `sloray-store` | SeaORM 实体与连接池 | `config` |
| `sloray-auth` | argon2id 密码、会话、Cookie | `store` |
| `sloray-setup` | setup token、数据库探测、迁移调用、systemd 单元 | `config` `auth` `migration` |
| `sloray-content` | 内容业务：文章、标签、修订、评论、搜索与计数（依据 2.2 新增） | `store` |
| `sloray-media` | 预签名上传、对象验证、派生图与对象任务（依据 2.2 新增） | `config` `store` |
| `sloray-web` | 路由、HTML 渲染、服务启动、日志 | 以上全部 |
| `sloray-migration` | 迁移 DDL | 无内部依赖 |
| `sloray`（根） | CLI 子命令分发 | 全部 |

**判断标准**：如果一个 crate 需要 `use` 上层 crate，说明职责放错了位置。
`config` 尤其不能依赖任何东西 —— 它是所有东西的最底层，
一旦它开始回指，循环依赖会立刻出现。

### 1.3 依赖与许可证门禁

`cargo deny check` 必须全绿（advisories / bans / licenses / sources）。
策略见 [`deny.toml`](deny.toml)，细则见第 4 节。

### 1.4 其它不可协商项

* **`unsafe_code = "forbid"`**：每个 crate 顶部都有 `#![forbid(unsafe_code)]`。
* **不允许 `*` 版本约束**，也不允许 git 依赖 —— 依赖只能来自 crates.io 官方索引
  （由 `[sources]` 强制）。
* **不允许 OpenSSL / native-tls**：统一走 rustls（由 `[bans]` 强制）。
* **迁移一旦发布就不再修改**，只能追加新迁移。
* **密钥绝不入库**：`api.env`、`config.json`、`setup-token` 都在 `.gitignore` 里。

---

## 2. 代码架构

### 2.1 分层规则

```
routes/ (HTTP 编排)  →  业务 crate  →  store  →  Postgres
```

* **HTTP 层只做编排**：解析请求、调用业务函数、构造响应。
  业务规则写进 `routes/` 是最常见的腐化方式。
* **`store` 不含业务规则**：它只知道表长什么样。
* **不把 SeaORM `Model` 直接序列化给外部**：
  那会把表结构固化成公开契约，并可能泄漏 `password_hash` 之类的列。
  对外一律用专门的 DTO。

### 2.2 什么时候该新建 crate

满足**任意两条**再建：

1. 它有一组稳定的、可以命名的职责（而不是"杂项"）；
2. 别的 crate 会想复用它，但不是全部；
3. 它需要的依赖集与现有 crate 明显不同（例如引入一个很重的第三方库）；
4. 拆出去之后，依赖方向变得更清楚而不是更绕。

否则就放在现有 crate 里当一个模块 —— **crate 数量本身不是目标**。

---

## 3. 治理原则

### 3.1 决策必须留痕

影响"以后怎么改"的决定（选型、放弃某个方案、引入新依赖类别）都要写 ADR，
放在 `docs/adr/`，编号连续，并在 `docs/adr/README.md` 里登记。

* ADR 一旦接受就**不改**。改了主意就写新的一条，在旧的上面标
  `Superseded by ADR-XXXX`。
* 说明**当时的约束与理由**，而不是罗列好处。ADR 的价值在于后人能判断
  "这个前提现在还成立吗"。

### 3.2 Postgres 是唯一权威源

搜索索引、缓存、计数、派生图都是**派生数据**，必须能从 Postgres 重建。

* **绝不双写**：需要同步到第二个存储时走 `outbox` 表 + worker。
* 引入任何新的存储前，先回答："它丢了能不能重建？" 答不上来就不许引入。

### 3.3 进程无状态

`serve` 进程不得在本地磁盘留下不可重建的状态。
目前**唯一的例外**是运行时配置文件 `config.json` —— 它是"部署期配置"
而非"运行期状态"，且 systemd 只对那一个目录开了写权限
（`ReadWritePaths`）。**不要扩大这个口子。**

### 3.4 失败要能回滚

* 迁移必须写成**向后兼容**的（见 5.2）。
* 部署脚本必须能在失败时切回上一版本
  （`deploy/scripts/test-activate-release.sh` 是它的可执行测试）。
* 破坏性操作（删列、删表）不依赖 `down` 回滚，而是写前向迁移修。

---

## 4. 依赖管理

### 4.1 添加依赖的流程

1. **先问能不能不加**。这个项目的依赖已经不少了；
   能用标准库或已有依赖实现就别加。
2. 加之前跑一次 `cargo deny check` 看它有没有许可证/公告问题。
3. 加进 `[workspace.dependencies]`，各 crate 用 `.workspace = true` 引用 ——
   **不要在单个 crate 里写死版本**，否则同一个库会出现多个版本。
4. 如果它引入了新的许可证，**必须同时更新 `deny.toml` 的 `allow` 列表**，
   并在 PR 描述里说明理由。
5. 内部 crate 的 path 依赖**必须带 `version`**，与 workspace 版本一致
   （cargo-deny 的 wildcard 规则会把"只有 path 没有 version"判为无约束依赖）。

### 4.2 许可证策略

本项目以 **MIT** 分发。允许：

* 宽松许可：MIT / Apache-2.0 / BSD-2/3-Clause / ISC / Zlib / 0BSD /
  Unlicense / CC0-1.0 / Unicode-3.0 / CDLA-Permissive-2.0 / BSL-1.0；
* **弱 copyleft：LGPL**（注意：Rust 多为静态链接，引入 LGPL 前需单独确认义务）。

**拒绝**：GPL / AGPL / SSPL 及任何强 copyleft。

### 4.3 检查命令

```bash
cargo deny check              # 全部（含 advisories，需要网络拉 RustSec 库）
cargo deny check licenses     # 只看许可证
cargo deny check bans         # 禁用列表与重复版本
```

`advisories` 里临时豁免某条公告时，**必须写 `reason`**，并在 PR 里说明何时移除。

---

## 5. 提交与审查

### 5.1 提交前必须跑（与 CI 一致）

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-architecture.sh          # 行数上限 + cargo deny
cd web && pnpm run check && pnpm run build
```

改动涉及数据库或部署时，额外跑：

```bash
deploy/scripts/test-activate-release.sh  # 发布切换与回滚
```

**"我本地跑过了"必须是真的跑过。** 报告结果时请贴出实际输出，
不要凭印象说"应该没问题"。

### 5.2 迁移的两阶段纪律（expand / contract）

回滚能回滚二进制，**不回滚数据库 schema**。所以迁移必须向后兼容：

| 阶段 | 做什么 | 例子 |
|---|---|---|
| expand | 只加，不改不删 | 加一个可空列、加一个索引 |
| 切换 | 新代码开始读写新结构 | 应用开始写新列 |
| contract | 确认稳定后，下个版本删旧结构 | 删掉废弃的列 |

违反的典型后果：发了"删旧列"的迁移 → 发布失败 → 回滚二进制 →
**旧代码立刻 500**，因为列已经没了。

### 5.3 什么时候必须写 ADR

* 引入或移除一个**依赖类别**（缓存、消息队列、搜索引擎……）；
* 改变**数据权威**的归属；
* 改变**部署拓扑**（实例数、进程边界、反向代理策略）；
* 推翻既有 ADR 的结论；
* 任何"以后想改回去会很贵"的决定。

小改动（加接口、修 bug、重构文件）不需要 ADR。

---

## 6. 快速参考

```bash
# 起服务（未配置也能起来，浏览器打开 /setup 完成初始化）
cargo run --bin sloray -- serve
cargo run --bin sloray -- setup-token

# 迁移
cargo run --bin sloray -- migrate up|down|status|fresh

# systemd 单元（cargo install 场景）
cargo run --bin sloray -- systemd print
sudo cargo run --bin sloray -- systemd install

# 全部门禁
./scripts/check-architecture.sh
```

| 想找什么 | 去哪 |
|---|---|
| 架构与取舍 | `docs/architecture.md` |
| 部署步骤 | `docs/deployment.md` |
| 中文检索设计 | `docs/search.md` |
| 媒体与对象存储 | `docs/media.md` |
| 为什么这么决定 | `docs/adr/` |
| 依赖合规策略 | `deny.toml` |
