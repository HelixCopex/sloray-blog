# ADR-0009：中文检索先用 Postgres FTS + pg_jieba，而不是上 Meilisearch

**状态**：已接受

## 背景

博客需要全文搜索，内容以中文为主。可选：

1. Postgres 自身的全文检索 + 中文分词扩展（pg_jieba / zhparser）；
2. 引入 Meilisearch / Elasticsearch 作为独立检索引擎；
3. MySQL 风格的 ngram 全文索引（本项目数据层已确定为 Postgres，不适用）。

## 决策

**优先方案 A：Postgres FTS + `pg_jieba`。** 明确准备两级退路：

| 方案 | 依赖 | 启用条件 |
|---|---|---|
| A. `pg_jieba` | 编译扩展（`postgresql-server-dev-N`） | 能在目标 PG 大版本上编译通过 |
| B. `zhparser` | 额外依赖 SCWS | A 编译失败 |
| C. **应用侧 `jieba-rs` 分词 + 内置 `simple` 配置** | 纯 Rust，**零扩展依赖** | A/B 都不可行 |

细节见 [`docs/search.md`](../search.md)。

## 理由

1. **事务一致性是最大的优势。** 搜索索引在同一个事务里更新，永远不会漂移。
   换成外部引擎就必须 outbox + worker 做最终一致，还要处理重复投递与重建。
2. **少一个常驻进程。** 单台 VPS 上，每多一个服务就多一份内存、监控、
   升级和故障面。ADR-0008 已经用同样的逻辑拒绝过 Redis。
3. **规模不构成理由。** 文章数以千计、写入以天计，GIN 索引完全够用。
4. **方案 C 的存在让方案 A 的风险可控。** 即使扩展编译不过，也只是把分词
   从数据库挪到应用层，索引依然可从权威列完整重建，架构原则不受影响。

## 影响

* `posts.search_vector`（tsvector）+ GIN 索引，由**触发器**维护。
  这里用触发器而非应用层写入，与"`updated_at` 用应用层"并不矛盾：
  `updated_at` 是业务语义，而 `search_vector` 是派生索引，
  它的不变式必须由数据库保证，否则手工修数据会让搜索静默失真。
* 需要提供一个全量重建函数（派生数据必须可重建，ADR-0005）。
* 用 `websearch_to_tsquery` 而非 `to_tsquery`：后者遇到用户输入的 `&`
  会直接抛语法错，等于把 500 暴露给搜索框。
* CI 与部署脚本里增加一条**分词自检**：`to_tsvector('jiebacfg', '分布式博客')`
  必须输出多个词位，否则说明扩展没生效。
* 抽象出 `SearchIndex` trait 作为换引擎的接缝，但**现在不实现第二种**。

## 重新评估的触发条件

1. 需要拼写容错、同义词、分面统计；
2. 中文分词质量不足且需要频繁调整自定义词典（方案 C 反而更适合这种场景）；
3. 数据量大到 GIN 索引写放大成为瓶颈（离得很远）。

真要换时，走 outbox + worker 投递，**绝不双写**（ADR-0005）。
# Superseded in part by ADR-0016（扩展安装前提与兜底维护方式）
