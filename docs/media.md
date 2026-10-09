# 媒体与对象存储

浏览器预签名直传，Rust 校验登记，worker 从 outbox 生成公开 WebP。
图片字节不经 API/Nginx 上传、不落 serve 本地盘。Postgres 保存权威元数据和任务，
原图私有对象是字节的权威副本，公开图可重建。见 ADR-0010、0017、0018。

## 必须分开的两个 bucket

R2 的自定义域名公开的是 bucket，`private/` 前缀不提供权限保护。
创建两个 bucket：

| 配置 | 用途 | 公开访问 |
|---|---|---|
| `s3.bucket` / SLORAY_S3_BUCKET | 原图，例如 sloray-originals | 禁止自定义公开域名和 r2.dev |
| `media_public_bucket` / SLORAY_MEDIA_PUBLIC_BUCKET | WebP，例如 sloray-public | 绑定媒体自定义域名 |
| `media_public_base_url` / SLORAY_MEDIA_PUBLIC_BASE_URL | 如 https://media.example.com | 只能指向派生图 bucket |

程序拒绝两个 bucket 相同，公开 bucket 与 URL 必须成对提供，S3 五项配置需完整。
应用凭据授权读写这两个 bucket；不在浏览器暴露 Secret Access Key。
公开派生 bucket 不收浏览器原图上传。
[Cloudflare 对公开 bucket 的说明](https://developers.cloudflare.com/r2/buckets/public-buckets/)
解释了该隔离的原因。历史已公开原图需关闭入口并清理旧缓存。

配置在 `/admin/settings` 的对象存储与图片区域填写；Secret/Token 留空保持原值。
CDN zone/token 成对可选；未配置时公开图使用 no-cache，避免无法撤回的长期缓存。
首次 setup 可跳过 S3，之后再启用媒体。

## 上传、重试与公开

1. 管理员选择 PNG/JPEG/WebP，前端计算 SHA-256。
2. POST `/api/v1/admin/media/presign`，声明类型、字节数与 hash。
   大小最大 10 MiB；已验证并处理完成的相同 hash 可直接返回媒体。
3. 新上传许可有效 10 分钟。签名 PUT 固定类型、SHA-256 校验、`If-None-Match: *`，
   对象 key 为私有 bucket 内 `private/YYYY/MM/<uuid>.<ext>`，禁止覆盖。
4. 浏览器直接 PUT 并显示进度；POST `/api/v1/admin/media` 登记 upload_id。
   服务端锁定许可，校验所有者、到期、HEAD 的大小/类型与实际 magic bytes，拒绝伪装 SVG。
   媒体行、完成状态与 media.generate outbox 在同一事务写入；重试登记返回同一媒体。
5. worker 读取完整私有原图，再验证真实 SHA-256 与大小，解码并重编码为无原图元数据的 WebP。
   生成 320/640/960/1280/1920 中不超过原宽度的尺寸；小于 320 的图片生成原宽度版本。
   限制边长 10000px、像素 3200 万、解码分配 128 MiB，CPU 工作使用阻塞线程。
6. 派生 key 为公开 bucket 内 `media/<id>/v1/<width>.webp`，条件创建且幂等重试。
   全部成功后标记 ready/verified，公共 `/api/v1/media/<id>` 才提供 URL。
   前端轮询至就绪，自动加入媒体选择器；可设封面或点击媒体复制 Markdown，再粘贴到正文。文章响应式封面带 srcset 和尺寸。

“登记成功”不等于“处理完成”；转换失败的媒体不会公开，任务保留并退避。
文章和独立页面编辑器通过“打开媒体选择器”上传或复用最近 100 张图片；媒体管理页也可直接上传。
点击已就绪图片复制 `![图片说明](<公开图片地址>)`，不会自动更改正文。图片说明可编辑；
浏览器拒绝剪贴板权限时显示可手动复制的 Markdown。未就绪图片不能复制或设为封面，可点击“刷新媒体”重试查看。
查看 worker journal 与 outbox 的任务 ID、次数、available_at 排查。
若原图已在私有桶、公开桶无 WebP 且任务 attempts=0，先检查 worker 是否运行：
`serve` 仅提供 API，不消费 outbox。本地推荐 `./scripts/dev-start.sh` 一起启动 API、
前端和 worker，共用同一配置；启动方式及日志路径见 [本地开发指南](development.md#3-启动-api前端并初始化)。
保存 S3/CDN 设置后须重启持续运行的开发 worker；生产的 `worker --once` 每次重新读取配置。
上传进度失败可重新选择文件重试，新许可不会覆盖原 key；未登记旧对象交 GC 处理。

上传失败时展开“上传错误详情”，可查看失败阶段（许可/原图直传/登记/处理）、
HTTP 状态、存储错误代码与请求编号（服务可读时）、上传编号、本站 Origin、存储端点
和 PUT 所需请求头名称。相同诊断写入浏览器 Console 的 `[media-upload]` 日志，
不包含签名 URL 的路径/查询参数、请求头值或存储错误响应原文。
原图 PUT 从浏览器直接到存储服务，Rust 服务端日志不会记录这次请求。

浏览器网络错误的 HTTP 状态为 0，不能从前端断定是 CORS、TLS、DNS 还是网络失败；
即使存储服务实际返回 403，缺少 CORS 响应头时浏览器也会隐藏它。
此时检查 Network 中的 OPTIONS/PUT 和 Console；不要把完整预签名 URL 贴到公开日志。

## 原图 bucket 的 CORS

允许站点精确 origin、PUT 和签名使用的头。示例需替换域名：

```json
[{
  "AllowedOrigins": ["https://blog.example.com"],
  "AllowedMethods": ["PUT"],
  "AllowedHeaders": ["content-type", "if-none-match", "x-amz-checksum-sha256", "x-amz-*"],
  "ExposeHeaders": ["ETag", "x-amz-request-id"],
  "MaxAgeSeconds": 3600
}]
```

以真实浏览器和供应商 API 验收 CORS 与签名校验。
CORS 不是读取权限控制，私有 bucket 仍须关闭所有公开入口。
本地开发的来源应精确加入 `http://localhost:4321` 或 `http://127.0.0.1:4321`；
两者是不同 origin，端口和协议也必须匹配，不能只允许线上域名或在 origin 后添加 `/`。
在私有原图 bucket 的 Settings → CORS Policy 配置上述规则；公开派生 bucket 的规则
不会应用到原图上传。对象凭据能读取 bucket 不代表已配置 CORS，桶配置需要另有管理权限。
公开派生域使用正确 Content-Type=image/webp；只有 worker 凭据能写。

## 删除、缓存与重建

管理员在媒体管理页删除，API 立即软删除，清空引用该封面的文章并增加文章版本，
事务内写入 media.delete。worker 删除私有原图与所有公开尺寸，再清理 CDN；失败可重试。
正文里手写的旧 URL 无法自动恢复语义，需在编辑器删除；对象删除后旧地址不可再取回。
已经在浏览器或第三方缓存中的副本无法承诺瞬时撤回。

启用 Cloudflare purge 时填 zone/token（仅需目标 zone 的 Cache Purge 权限）。
投递按 URL 清理，失败指数退避至最多 900 秒，未成功不标记完成。
动态 HTML 始终重验证，CDN 必须尊重 no-store/no-cache；不启用 Cache Everything。
派生版本目录变更需保留旧对象直到新 URL 切换，不覆盖同一 immutable key。

```bash
sloray worker --once
sloray rebuild-media      # 重新投递所有未删除原图的派生任务
sloray media-gc           # 默认仅列候选
sloray media-gc --apply   # 删除 24h 前无登记/无活跃许可的私有孤儿
```

生产 worker timer 每约 5 秒触发；不要依赖手工执行才生成图片。
GC 只列本站私有前缀，保护已登记媒体与尚未过期的许可；默认不删除对象。
备份必须另覆盖私有原图，不能仅备份公开派生图或 Postgres。
本地协议模拟器测试与真实浏览器覆盖的边界见 [verification.md](verification.md)，
不等同于已连接线上 R2/Cloudflare。
