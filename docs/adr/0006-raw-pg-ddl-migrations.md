# ADR-0006：迁移用原生 Postgres DDL，且与 API 启动解耦

**状态**：已接受

## 背景

SeaORM 提供了 `sea-orm-migration` 的 schema DSL
（`Table::create().col(ColumnDef::new(...))`）。
另外，SeaORM 生态的常见做法是在应用启动时调用 `Migrator::up(...)`。

## 决策

1. 迁移是**独立二进制** `sloray-migrate`，由部署脚本在重启应用**之前**执行一次
   （`deploy/scripts/rollout.sh`）。
   `sloray-api` 启动时**不**执行任何迁移。
2. 迁移内容使用**原生 Postgres DDL 字符串**，每条语句一次
   `execute_unprepared`，而不是 sea-query 的 DSL。
3. 整个迁移过程包在一个事务里，并先用
   `pg_advisory_xact_lock(0x736C6F726179)` 取咨询锁（`0x736C6F726179` 即 "sloray"）。

## 理由

**为什么迁移独立于应用启动**：应用启动时迁移，在多实例滚动发布下会产生
并发迁移竞争 —— 两个实例同时启动、同时尝试改 schema。而且它把
"改 schema"和"启动服务"耦合在一起，回滚时无法分离。
（ADR-0001 里说 API 是无状态进程，启动时改 schema 恰恰打破了这一点。）

**为什么用原生 DDL 而不是 DSL**：

* 本项目已决定 Postgres 是唯一权威源（ADR-0005），没有多后端需求，
  DSL 的主要卖点（跨库可移植）在这里没有价值；
* sea-query 1.0 是刚发布的大版本，组合主键改用
  `Table::primary_key(&mut IndexCreateStatement)`、外键列要求 `IdenList` 等
  API 都有变动。用 DSL 生成的 DDL 在**真正跑过 Postgres 之前是不可验证的**，
  而原生 SQL 是任何 Postgres 从业者一眼能审阅的 —— schema 变更的 review 成本最低；
* 可以用上 PG 原生能力：部分索引（`WHERE deleted_at IS NULL`）、
  `CHECK` 约束、`bytea`、复合主键，不必和类型系统较劲。

**为什么用事务级咨询锁**：会话级 `pg_advisory_lock` 绑定在某条连接上，
而连接来自连接池、可能换手，`unlock` 有可能落到另一条连接上；
事务级锁在事务结束时自动释放，进程崩溃也不会留下死锁。

## 影响

* **迁移的 DDL 必须被真实执行过。** 已在 Postgres 17.2 上实测通过
  （`up → status → down 1 → up`，12 张表全部创建）；
  CI 里另有专门的 `migrations` job 在 Postgres service container 上持续守护。
  没有这一步，迁移就是不可验证的代码 —— 而这正是选择原生 SQL 的理由：
  它一眼可审阅，无需先跑起来才知道生成了什么 DDL。
* 已发布的迁移只追加、不修改（文件名就是记录在 `seaql_migrations` 里的标识，
  由 `DeriveMigrationName` 从 `file!()` 的文件名派生）。
* 单元测试覆盖了两条容易犯的错：`DOWN` 必须是 `UP` 建表顺序的严格反序；
  每张表都必须有主键（否则将来无法做逻辑复制的只读副本）。
