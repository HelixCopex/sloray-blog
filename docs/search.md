# 中文检索

Postgres 是文章与全文检索的权威源。`SearchIndex` trait 为将来替换实现保留接缝，
当前实现直接查询 PostgreSQL，不引入第二个搜索存储。决策见 ADR-0009 与 ADR-0016。

## 当前实现

检索迁移探测已经安装的 `public.jiebacfg`、`public.zhcfg`，并用“分布式博客架构”验证
能生成多个词。探测成功使用数据库扩展；否则自动使用应用 `jieba-rs` + 内置 simple。
应用随程序携带默认词典，无需服务器安装中文 C 扩展或改全局默认检索配置。

`search_settings` 单行记录 engine（database/app）和 regconfig 名称。
标题 A、摘要 B、正文 C，加权合并为 search_vector；未删除文章有部分 GIN 索引。
公开查询仍过滤 published、发布时间已到和未软删除。

数据库模式由触发器分词；应用模式在文章编辑的同一事务中写入分词和源文本摘要。
直接 SQL 修改权威文本时触发器使旧分词失效，worker 重新生成，不会继续使用旧结果。
worker 每批处理最多 50 篇；积压时新文本可能短暂无法搜索，正文读取仍正常。

查询入口 `GET /api/v1/search?q=...&cursor=...&limit=...`。
关键词 1–300 字，默认 20、最多 50 条。数据库 `websearch_to_tsquery` 解析引号短语、
OR 与 -排除；应用模式保留操作符并先分词文本片段，不拼接用户输入到 SQL。
普通 AND 查询不足一页时尝试 OR 扩展，返回 relaxed 提示；引号/排除/显式 OR 不再放宽。
排序为 `ts_rank_cd`、发布时间、UUID，三者降序 keyset。
游标绑定关键词、扩展模式、engine 和 config，配置变更后要求重新搜索。
这不是跨请求的数据库快照，编辑正文造成 rank 变化时分页结果可能移动。

## 重建与配置切换

```bash
sloray rebuild-search
```

数据库模式调用 `rebuild_posts_search_vector()`；应用模式清空旧分词后分批从权威文本生成。
重建可重复执行，不依赖磁盘缓存。更改 search_settings 后必须重建；不要仅修改全局
`default_text_search_config`，所有查询都显式指定配置。

切到已安装扩展前先验证中文分词。维护窗口内更改配置并使旧向量失效：

```sql
BEGIN;
UPDATE search_settings SET engine='database',config='public.jiebacfg' WHERE singleton;
UPDATE posts SET search_source=NULL,search_vector=NULL;
COMMIT;
```

然后执行 rebuild-search；切回应用模式用 `engine='app',config='pg_catalog.simple'`，
同样使旧向量失效并重建。SQL 表名及字段以迁移为准；无需安装独立“sloray-migrate”二进制。

## 可选扩展安装

`deploy/scripts/install-pg-jieba.sh` 按上游 CMake 和递归子模块流程构建。
`PG_CONFIG` 必须指向目标数据库大版本对应的开发工具，安装后显式在应用库创建扩展，
并用 SQL 验证中文词元；安装不是启动前提。

```bash
sudo PG_CONFIG=/usr/lib/postgresql/17/bin/pg_config \
  deploy/scripts/install-pg-jieba.sh
```

`zhparser` 作为已部署环境的另一种配置可被探测；仓库不自动安装或修改词典。
原生扩展的字典路径、服务器 ABI 和运维权限必须在目标宿主验证。
本地无扩展模式、触发器失效与重建、中文匹配、排序/游标和压测由
`scripts/smoke-search.py` 执行，实际测量见 [verification.md](verification.md)。

## 扩展边界

不做复杂查询语言、自定义词典 UI 或多语言拼写纠错。
若规模证明 PG FTS 的延迟或功能不足，再写 ADR 引入 outbox 到独立索引；
不能同时写两个权威存储。PG trigram 模糊搜索不是当前公开契约。
