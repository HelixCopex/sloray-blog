# ADR-0021：响应式 Dock、首页编排与站点标识

- 状态：已接受
- Dock 可见文字与目录布局部分 Superseded by [ADR-0022](0022-icon-dock-following-toc-and-background.md)。
- 日期：2026-10-10
- 部分取代 ADR-0020 的导航位置与首页区块限制；数据主题和原生群岛边界继续有效。

## 背景与约束

维护者要求手机底部可展开 Dock、桌面侧栏，以及截图中的玻璃表面和柔和泛光。
交互参考 Material 3 与 BewlyCat 的浮动容器、悬停放大、主题切换和独立操作入口。
博客需要长时间阅读和写作，不能靠隐藏文字、持续动画或外部图标字体实现视觉效果。
首页需要按纵向顺序编排，允许区块内放置可独立刷新的小组件。
现有 JSON 主题包、Postgres 权威、Rust 恢复页面与 Astro SSR 的边界必须保留。

## 决策

1. Astro 页统一使用桌面侧栏和手机底部 Dock。移除独立首页按钮，站点名称链接返回首页。
   桌面展示全部链接；手机折叠状态展示前两个导航链接、外观、顶部与展开操作，展开展示全部链接。
   保留可见文字、48px 目标、焦点、Escape/外部点击收起、滚动与安全区域。
   无 JavaScript 时展示全部链接。旧主题 navigation 字段继续读写，但不改变响应式导航规则。
2. 官方 Material Symbols Rounded SVG 打包到本地 sprite，保留 Apache-2.0 许可证。
   用 CSS 实现按压、涟漪、悬停放大/旋转/泛光；遵守 prefers-reduced-motion，颜色即时切换以避免中间帧对比度问题。
   不引入 Vue、动画框架或 BewlyCat 源码。BewlyCat 仅作为交互设计参考。
3. ThemeLayout 增加可选 home_rows，空数组仍按 home_blocks 解释。首页外层为线性列表；
   有且仅有一个文章区块、最多一个引言、最多 12 行；每个组件域容纳 1–4 个受控小组件。
   支持便笺、标签、公开页面与 RSS；组件内容和标题只作为文本。后台提供按钮排序和组件表单。
   配置沿用主题库/启用快照/版本锁，保存不发布；无任意模板或代码入口。
4. 文章列表/正文同步 SSR；组件首次使用 HomeWidget server:defer，局部刷新请求 Astro 的
   /fragments/widgets，读取当前发布快照，重新渲染一个组件。刷新不替换主页面或文章列表。
   失败保留已有内容并可重试；页面离开时取消请求与观察器。
5. 第七条 expand 迁移增加 site_identity 单例。站点名称与 favicon 是编辑期公开内容，
   不保存到 config.json。Rust 设置页用独立表单提交，管理员、CSRF 和版本条件更新；
   更改在同一数据库事务触发 CDN purge outbox，含 RSS。
   Favicon 接受同源 / 路径或 HTTPS 地址，留空用内置 SVG；可使用既有媒体公开地址。
6. 阅读辅助包括字号偏好、自动目录、进度、专注阅读与代码复制；写作增加常用 Markdown 插入和字数。
   本地存储只保存读者外观/字号偏好，不承载文章草稿或权威站点配置。

## 兼容与回滚

新增表不影响旧二进制，部署失败仍可按原流程回滚，不能回退数据库 schema。
旧主题包无需导入转换，导航表现按新响应式规则统一。home_rows 是可选的公开契约扩展；
回滚到不识别该字段的旧程序前，应启用仅含 home_blocks 的旧主题或恢复默认。
旧程序拒绝未知主题字段时 Astro 会使用内置主题，新增首页编排不能在旧程序上继续编辑。
Rust 登录/setup/设置页继续自包含，不受公开自定义 CSS 影响。

参考：[Material 3](https://m3.material.io/)、[BewlyCat Dock](https://github.com/keleus/BewlyCat/blob/main/src/components/Dock/Dock.vue)、
[Material Symbols](https://github.com/google/material-design-icons)、[Astro server islands](https://docs.astro.build/en/guides/server-islands/)。
