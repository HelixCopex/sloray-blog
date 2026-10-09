# ADR-0016：可用扩展优先与可重建应用分词兜底

* 状态：已接受
* 日期：2026-10-09
* 取代 ADR-0009 中“必须先尝试安装扩展才允许启用应用兜底”的部署前提

目标部署允许没有社区 C 扩展的 Postgres，CI 的 PG 16/17 镜像也未安装中文
扩展。应用初始化不能要求数据库超级用户或联网编译第三方 C/C++。
迁移优先检测已有 public.jiebacfg、public.zhcfg，并验证中文子词命中。
两者都不可用时自动选择 jieba-rs + pg_catalog.simple，状态保存在数据库，
所有 API/worker 使用同一模式。不会自动修改默认 text search config。

新增的分词文本与 tsvector 都是派生数据，原始文章仍为唯一权威源。
数据库模式由触发器同步维护；应用模式由内容事务写分词文本，再由触发器
计算带权重向量。触发器比较权威字段指纹，绕过应用的写入使旧向量失效，
worker 重建失效项；搜索因此可能短暂漏查，但不会返回失真的旧索引。
rebuild-search 从原文全量重建。数据库模式也支持 SQL 重建函数。

查询始终通过 websearch_to_tsquery；应用模式先分词普通查询片段，保留
引号、OR 与排除操作符。普通 AND 查询不足一页时可以整体切换 OR，游标
固定查询与模式，防止两种查询的分页相互混入。
索引使用普通 GIN，当前规模允许事务内建立。大表改用并发索引需另行迁移，
不能在已有事务咨询锁中直接使用 CREATE INDEX CONCURRENTLY。

参考：[Postgres 查询构造](https://www.postgresql.org/docs/current/textsearch-controls.html)、
[jieba-rs API](https://docs.rs/jieba-rs/0.11.0/jieba_rs/struct.Jieba.html)。
