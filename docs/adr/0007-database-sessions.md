# ADR-0007：会话存数据库表，而不是 JWT

**状态**：已接受

## 背景

后台需要登录态。常见两种做法：无状态 JWT，或服务端会话。

## 决策

使用 `sessions` 表 + **不透明随机 token**：

* 客户端只拿到 token 的随机值，放进 `httpOnly` + `Secure` + `SameSite=Lax` 的 Cookie；
* 数据库里存的是 `token_hash`（`bytea`，唯一索引），**不存明文**；
* 密码用 `argon2id` 哈希。

## 理由

1. **可撤销。** 退出登录、封禁账号、修改密码后让所有会话失效 ——
   这些都是后台的常规需求，而 JWT 要做到这一点必须引入黑名单，
   那等于把状态又加回来了，却失去了 JWT 唯一的优势。
2. **天然多实例友好。** 任意 API 实例都能校验同一个会话，
   不需要"粘性会话"（sticky session），也不需要共享密钥分发。
   JWT 的无状态性在单机部署下没有实际收益。
3. **token 泄漏面的差别。** 数据库泄漏时，存哈希意味着会话不能被直接接管。
4. **可以自然过期与轮换**：`expires_at` 有索引，清理是普通的 `DELETE`。

## 影响

* 会话表需要一个定期清理任务（删除 `expires_at < now()` 的行），
  放在 worker 里，用 `SKIP LOCKED` 或简单的批量 `DELETE ... LIMIT`。
* Cookie 属性：`httpOnly`（JS 读不到）、`Secure`（仅 HTTPS）、
  `SameSite=Lax`。配合 ADR-0002 的同源部署，这些属性都能正常生效。
* 写操作需要 CSRF 防护（`SameSite=Lax` + double-submit token），
  因为浏览器会自动带上 Cookie。
