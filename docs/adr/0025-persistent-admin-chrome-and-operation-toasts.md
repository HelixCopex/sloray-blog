# ADR-0025：后台持久导航与操作通知

- 状态：已接受
- 日期：2026-10-10
- Superseded by ADR-0026：第 1 项的正文快照布局处理，后台持久 Tab 继续适用。
- Supersedes ADR-0024 第 3 项中图片尺寸读取失败时阻断保存的处理；其余决策继续适用。

维护者报告 Zen 在切页动画期间把主题和媒体内容压缩，要求后台 Tab 留在动态内容之外，
页面不显示阅读进度，文章进度只显示醒目的前景条，操作反馈统一为带颜色的 Toast。
正文图片的远端读取失败也不能让已选择媒体库封面的文章无法保存。

1. 后台导航由 AdminLayout 提供独立 chrome 插槽，位于 main 外并持久化。
   局部切页只同步当前 Tab，保留导航节点。设置仍使用既有完整导航边界。
   page-content 的 View Transition group 不做浏览器默认尺寸插值，快照保持自动高度、
   contain 与顶部对齐，正文继续使用 ADR-0024 的 MD3 滑移和透明度曲线。
2. 管理后台 Dock 使用本地图标集中的 Material Symbols Rounded dns（服务器）图标。
   独立页面保留目录但不渲染阅读进度；文章进度轨道透明，前景使用主题主色。
3. 图片保存先复用 Postgres 媒体元数据（各派生 URL 对应原图比例），再复用已解码
   的编辑器图片，最后由浏览器测量。单张远端图片无法读取时保存其原始正文，显示
   警告 Toast，恢复连接后再保存或手工补全尺寸。鉴权、预览解析或保存接口失败仍
   保留表单并显示错误 Toast。服务器不为此请求任意外部图片。
4. Astro 页面和 Rust 自包含页面共享同一原生 Toast 实现及样式。成功为绿色，
   信息为蓝色，提醒为黄色，错误为红色，同时保留文字和 alert/status 语义。
   通知通过 manual popover 显示在顶层，可关闭，悬停/聚焦暂停自动消失，最多四条。
   跨局部导航保留通知节点；完整导航后的保存反馈使用一次性 sessionStorage 消息。
   通知内容只用 textContent，不接受 HTML；服务端既有 notice 在启用 JS 后转换为 Toast。
   无 JS 时保留服务端文本反馈。上传诊断仍可按需展开查看，内容状态及确认对话框
   不作为 Toast 消息。编辑与身份权限、CSRF 和并发版本校验保持既有接口约束。

本次不增加依赖、数据库迁移或新的权威数据源。宽高仍随 body_md 及修订保存。
不阻断远端图片失败意味着该图片暂用 ADR-0024 的稳定 16:9 容器，直到尺寸补齐。

快照布局参考浏览器规范的
[View Transition old 替换元素及 object-fit](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Selectors/::view-transition-old)。
