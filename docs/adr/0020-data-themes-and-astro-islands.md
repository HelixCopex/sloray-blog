# ADR-0020：数据主题包、Material 3 与 Astro 原生群岛

- 状态：已接受
- 预览背景图片资源部分 Superseded by [ADR-0022](0022-icon-dock-following-toc-and-background.md)。
- 导航与首页编排部分 Superseded by [ADR-0021](0021-responsive-dock-and-home-composition.md)。
- 日期：2026-10-09

## 背景与约束

站点通常由一人维护，需要从后台调整颜色、排版和页面布局，无需修改源码或重新构建。
管理功能还必须能在错误主题、数据库故障或 Astro 不可用时恢复。
Postgres 是运行期数据唯一权威源，serve 不得保存模板、插件或上传文件到本地；
部署配置 config.json 不能承载编辑期主题状态。正文、列表和 SEO 仍需服务端渲染。
维护者选择 JSON＋CSS 主题包，不要求执行第三方代码。

## 决策

1. 主题插件是版本化 JSON 数据包：颜色方案、系统字体族、宽度、导航位置、文章列表形态、
   列数、首页区块顺序与文字，以及最多 64 KiB 自定义 CSS。schema_version=1；拒绝未知字段、
   JavaScript、HTML 模板和任意代码入口。通过管理员 API 统一校验，前端导入/导出同一契约。
2. 新增 themes 和 theme_settings 两张表。主题库可编辑；启用操作复制完整包成为发布快照。
   编辑已启用的主题不会隐式发布。库记录与单例启用记录分别使用乐观版本控制；启用同时核对已预览的库版本，启用/删除
   使用事务与相同锁顺序；启用主题不可删除。恢复默认清空快照，保留库。发布触发 outbox purge。
3. 自定义样式只加载到公开站点，后台、登录、setup 保持内置 Material 3 样式。
   基础样式置于 CSS cascade layer，用户样式与主题变量可以可靠覆盖默认值。
   CSS 中 HTML 解析器的 `<` 转义为 CSS 转义，避免闭合 style 标签。
   草稿预览使用无权限 sandbox iframe、脚本禁用和独立 CSP；预览不访问外部资源。
   线上 CSS 由可信管理员维护，可引用外部资源；它不是不可信代码插件的安全执行环境。
4. 使用 Material 3 语义颜色、字号、圆角、48px 操作目标、焦点和响应式导航。
   Material 图标用本地 SVG symbol，共享 Astro 与 Rust 页面，无远程图标字体依赖。
   附带 Google 图标来源、Apache-2.0 许可证；项目代码仍为 MIT。
5. 静态结构用 Astro 组件复用，交互用有独立生命周期的原生自定义元素和 Astro 打包脚本。
   主题工作台、可选首页页面目录使用 server:defer；正文/文章列表保留同步 SSR。
   不为了简单表单引入新的 UI 框架。Astro 的 ClientRouter 保持页面导航；编辑页保留完整离开确认。

## 后果与边界

可直接导入、预览、启用、恢复和备份主题，且无新的 Rust/npm 依赖。
页面区块来自受控组件集合，不允许主题创造可执行 Astro 模板。可增加新 schema 版本扩展能力，
不能把旧版主题静默解释为脚本插件。错误的公开 CSS 可以通过 /login → /admin/themes 恢复。

Astro 服务端群岛使用构建产物内的密钥；同一构建包在所有 SSR 实例部署时一致。
跨构建仍在打开的旧页面可能需要刷新才能读取群岛。需要跨构建密钥一致时可以按 Astro 文档
在构建环境设置 ASTRO_KEY，不能把密钥提交到仓库。此变更不改变部署拓扑或会话存储。

测试覆盖包边界、鉴权/CSRF、发布快照、版本冲突、HTML/CSS 隔离、导入导出、群岛加载、
360/768/1440px 布局和 WCAG AA 审计；数据库迁移与备份恢复包括两张新表。

参考：[Astro 群岛](https://docs.astro.build/en/concepts/islands/)、
[服务端群岛](https://docs.astro.build/en/guides/server-islands/)、
[Material 3](https://m3.material.io/)、
[Material Icons](https://github.com/google/material-design-icons)。
