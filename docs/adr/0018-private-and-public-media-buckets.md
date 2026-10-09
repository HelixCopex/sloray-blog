# ADR-0018：原图与公开派生图使用不同 bucket

状态：Accepted，2026-10-09。取代 ADR-0017 中以路径前缀表示原图私有的部署约定。

## 约束与问题

原图可能含位置等元数据。R2 自定义域名开启的是 bucket 级公开访问，`private/` 前缀本身不是访问控制。如果把原图与派生图放在同一个公开 bucket，知道对象 key 的访问者仍可能读到原图。

## 决定

`s3.bucket` 只存原图，关闭自定义域名与 r2.dev 公开访问；`media_public_bucket` 只存 worker 重编码的 WebP，并绑定 `media_public_base_url`。两个 bucket 必须不同；公开 bucket 与地址必须成对配置。API 签名 PUT 始终指向私有 bucket，worker 的派生写入与删除指向公开 bucket。

## 后果

运维需要创建两个 bucket，应用凭据需要读写二者。已经公开过的原图需要关闭公开入口并清理历史缓存；仅升级程序无法撤回外部缓存。Postgres 仍记录权威媒体信息，派生图仍可由原图重建。

依据：[Cloudflare R2 public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/)。
