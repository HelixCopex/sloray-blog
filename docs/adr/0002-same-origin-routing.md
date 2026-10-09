# ADR-0002：单域名 + 反向代理路径分流

**状态**：已接受

部署实现 Superseded by [ADR-0013](0013-nginx-reverse-proxy.md)；同源原则保持。

## 背景

前后端分离之后，浏览器要同时访问 Astro 页面和 Rust API。有两种方式：

1. Astro 与 API 各占一个域名/端口，浏览器跨域调用 API；
2. 只暴露一个域名，由反向代理按路径分流。

## 决策

采用方案 2。Caddy 按路径分发（见 `deploy/Caddyfile`）：

| 路径 | 上游 |
|---|---|
| `/api/*` | `sloray serve`（8080/8081） |
| `/healthz`、`/readyz` | `sloray-api`（探针暴露方式后来有修订，见文末） |
| `/_astro/*` | Astro 产物（长缓存） |
| 其余 | `sloray-web`（4321） |

## 理由

1. **没有 CORS。** 跨域意味着要维护一份允许来源列表、处理预检请求、
   正确配置 `Access-Control-Allow-Credentials`。同源让这些问题根本不出现。
2. **会话 Cookie 变成第一方 Cookie。** 跨域场景下 API 的 Cookie 属于第三方
   Cookie，正被各浏览器持续收紧（Safari ITP、Chrome 逐步淘汰），
   而登录态依赖它。这是方案 1 的致命问题。
3. **Astro 不必当会话代理。** 它不需要转发 Cookie、不需要再鉴权一遍，
   职责更纯粹：拿数据、渲染。
4. **探针不必暴露公网。** `/healthz` 和 `/readyz` 只对 `127.0.0.1` 开放。
   （这一条后来被修订，见文末。）

## 影响

* API 侧**不需要**配置 CORS 中间件。
* 部署时两个进程都必须监听回环地址（`127.0.0.1`），不能监听 `0.0.0.0`。
* 浏览器端的 `fetch` 使用同源相对路径 `/api/v1/...`（见 `web/src/lib/api.ts`）。
* 服务端 SSR 走内网直连（`SLORAY_API_INTERNAL_URL`），绕开 Caddy 一跳。

## 修订（2026-10）：探针改为对公网开放

原先的理由 4 是"探针不必暴露公网"。后来按需求**改为对公网开放**，
好让外部探测系统直接使用，不必开 SSH 隧道。

**为什么这个改动是可接受的**：`/readyz` 的响应体从一开始就是脱敏的——
只返回 `status` 与依赖名（`{"checks":[{"name":"postgres","ok":false}]}`），
不含连接串、主机名或 SQL。这不是事后补的，而是初始实现里就有测试守护的
不变式（`crates/api/tests/health.rs::readiness_does_not_leak_connection_details`）。

**"仅本机"这个限制本身与 ADR-0002 的核心决策无关**——本 ADR 要保的是
"单域名同源分流"，探针的可见范围只是这条路由表上的一个可调参数。
因此这里不新开一条 ADR，而是就地记录修订：

* `deploy/Caddyfile` 里的 `@not_local` 限制已移除；
* 若将来要收回，把 `handle @probes` 块删掉即可（请求会落到 Astro 分支返回 404），
  不需要改 API 侧任何代码。
