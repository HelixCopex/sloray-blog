# ADR-0017：隔离媒体流水线与可恢复对象操作

* 状态：已接受
* 日期：2026-10-09

媒体签名、对象操作和图片编解码有稳定职责，HTTP 与后台任务均需调用，
且 AWS SDK/image 的依赖明显不同。依据 AGENTS.md 2.2 新增 sloray-media，
依赖 config/store，不把对象存储或编解码放入 HTTP handler 或 store。

预签名 PUT 写入 private/ 原图前缀，签入 If-None-Match:*、Content-Type 和
SHA-256 校验和。上传意图存在单独表中，登记时校验所有权、过期、HEAD 实际
大小与有界 Range 读取的 magic bytes。原图不经过 API 中转，验证读取的
少量字节是原文“字节永不经过进程”的必要例外。
只有 worker 验证完整 SHA-256 并安全解码后才公开重编码 WebP，原图始终私有。
未配置对象存储时写作与文字发布可独立工作。

media 元数据与 outbox 任务在同一事务提交。对象写入、删除、CDN purge
是可重复执行的投递，失败保留任务并退避，不将错误当作完成。
生成版本进入变体键，版本内只条件创建对象，规格变化用新键。
软删除立即停止公开引用，异步删除对象并请求 CDN purge。
孤儿 GC 默认只输出候选，显式 --apply 才删除超过 24 小时且无登记/活跃意图
的 private/ 原图，不扫描其它 bucket 前缀。

使用官方 AWS SDK 的 SigV4 与 rustls；显式凭据不读取实例元数据或凭据链，
无需 aws-config。图片类型限定 PNG/JPEG/WebP，不添加 infer 或 EXIF 解析器：
已有 image 提供格式识别，重编码丢弃元数据，可通过构造包含元数据的样本验证。

Superseded by ADR-0018（原图与公开派生图的 bucket 隔离约定）。
