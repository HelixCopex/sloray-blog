# ADR-0013：部署反向代理使用 Nginx

* 状态：已接受
* 日期：2026-10-09
* 补充：ADR-0002 的同源路径分流；取代其中的 Caddy 部署实现

## 约束与决策

目标主机使用 Nginx。继续安装 Caddy 会重复监听 80/443、维护第二套
TLS 配置，并使现有运维流程无法复用。因此只保留 Nginx 配置示例，
初始化脚本安装发行版的 Nginx，不添加 Caddy 软件源或服务。

保持单域名同源分流：`/api/`、探针、`/setup`、`/login`、`/logout`
与 `/admin/settings` 到 Rust；内容页面和后台写作页面到 Astro。
Nginx 必须覆盖真实 IP 与转发协议头。应用仍只监听回环地址。

## 后果

证书签发、续期与域名配置由目标主机的 TLS 运维流程负责。
应用发布不自动覆盖 Nginx 的站点配置或证书。

开源 Nginx 使用被动失败检测，而非 Caddy 的主动 `/readyz` 检查。
GET/HEAD 可在连接错误和 502/503/504 时切换到另一个 API 上游；
不启用 `non_idempotent`，避免重放已经送出的 POST/PATCH 请求。
发布脚本逐个重启并检查就绪，不能据此承诺所有写请求零失败。

动态 HTML 暂不在 Nginx 层缓存，避免撤回后的正文残留及后台内容泄漏。
如引入外部 CDN 缓存，必须同时实现并验证其 purge 适配器。
带内容哈希的 `/_astro/` 文件仍使用 immutable 缓存。

参考：[Nginx upstream](https://nginx.org/en/docs/http/ngx_http_upstream_module.html)、
[proxy_next_upstream](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_next_upstream)。
