# ADR-0022：纯图标 Dock、随动目录与主题背景

- 状态：已接受
- 日期：2026-10-10
- 部分取代 ADR-0021 的 Dock 可见文字和阅读目录布局，以及 ADR-0020 的预览图片资源限制。

## 背景与约束

维护者进一步指定圆形纯图标 Dock、高对比度、Starlight 风格的随动目录、超宽屏限制，
并要求移除默认渐变，参考 BewlyCat 提供背景与模糊度设置。
内容和主题仍以 Postgres 为权威，不能为壁纸引入浏览器持久缓存或本地文件存储。
目录必须与清洗后的正文标题一致，JavaScript 不可用时仍可阅读和跳转。

## 决策

1. Dock 使用无可见文字的圆形图标目标，保留 aria-label、title、焦点和 48px 最小尺寸。
   活动项使用前景/表面反色及局部泛光；桌面纵向胶囊，手机底部横向胶囊并可展开。
   站点品牌仍是返回首页的链接，不新增独立首页按钮。Rust 设置页同步采用图标导航。
   尊重减少动态效果和强制颜色偏好，不使用持续装饰动画。
2. 文章和独立页面共用 SSR 目录。Astro 的同一次 Markdown 渲染给出标题文本、深度和唯一锚点；
   桌面右侧列表 sticky、内部可滚动，正文最多 760px，整篇区域最多 1120px。
   手机使用 sticky details 菜单，选择章节自动收起，Escape 收起并返回菜单焦点。
   requestAnimationFrame 同步当前标题，ResizeObserver 处理图片/字号变化，
   MutationObserver 处理正文标题更新；局部换页时清理全部监听器。
3. 默认使用纯色背景，去除全页和引言渐变；保留受控玻璃表面与局部泛光。
   ThemePackage 增加可选 background：image_url、blur（0–40px）、overlay（0–100%）、
   panel_blur（0–40px）。旧包未提供时使用纯色、零背景模糊、70% 遮罩和 24px 面板模糊。
   图片只接受同源绝对路径或 HTTPS 地址，可使用已有媒体库公开派生图；
   由浏览器加载，不在服务端抓取、缓存或复制。主题库保存、启用快照、版本与 purge 流程保持一致。
   背景丢失仍有纯色表面；正文使用 95% 表面遮罩保证可读性，专注阅读隐藏背景。
4. 主题预览继续使用无权限 sandbox、无脚本和独立 CSP，图片资源只额外放行配置的背景 URL。
   其他外部 CSS、字体和连接仍禁用。HTML 与 CSS 字符串分别转义，不能利用 URL 闭合样式。
   配置留空即可恢复纯色，不需要改源码或数据库迁移。

## 兼容与回滚

背景作为已有 JSON 主题的可选字段，不新增表或依赖，不改变部署拓扑。
回滚到不识别 background 的旧程序前，恢复默认或启用无该字段的旧主题。
自定义 CSS 仍由可信管理员负责；后台、登录和 setup 不应用公开主题或背景。

参考：[Starlight](https://starlight.astro.build/)、
[BewlyCat 背景](https://github.com/keleus/BewlyCat/blob/main/src/components/AppBackground.vue)、
[BewlyCat Dock](https://github.com/keleus/BewlyCat/blob/main/src/components/Dock/Dock.vue)。
