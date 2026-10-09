# ADR-0015：数据库共享限流与单二进制后台任务

* 状态：已接受
* 日期：2026-10-09

双 API 实例各自使用进程内限流会把全局配额变成两份，滚动重启还会清空窗口。
登录、评论、浏览和点赞使用 Postgres 固定窗口原子 upsert，按数据库内随机盐
脱敏后的客户端地址计数。只有 loopback 上游的 X-Real-IP 被信任，Nginx 必须
覆盖该头。共享 NAT 可能共用配额；数据库不可用时写请求失败关闭。

浏览以 UTC 日、文章和脱敏 IP + 有界 User-Agent 去重，不宣称精确独立访客。
同一 NAT 与浏览器可能合并，变更浏览器可能重复；这是避免永久跟踪 Cookie
的取舍。点赞只对登录用户开放，以用户/文章唯一约束保证幂等。
聚合计数与权威记录在同一事务写入，并提供 rebuild-stats 重建命令。

后台任务继续使用唯一的 sloray 二进制，新增 worker 子命令。
worker --once 可由 systemd timer 调度，每次重新加载配置；常驻模式供开发。
到期发布用 FOR UPDATE SKIP LOCKED，多 worker 重叠不会重复发布。
评论正文暂按纯文本展示，保留 Markdown 源字段，不允许原始 HTML 执行。

HTML 管理表单与 JSON 写请求都采用 double-submit CSRF token。
密码计算移到阻塞线程，避免 Argon2 占用异步运行时线程。
