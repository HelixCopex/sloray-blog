# 主题插件

访问 /login 登录管理员账号，然后在后台进入“主题”（/admin/themes）。
主题为 JSON＋CSS 数据包；不加载第三方 JavaScript 或 Astro 模板。

## 操作

- 新建主题：填写名称、颜色、字体与布局；预览在独立 iframe 内实时更新。
- 导入主题包：选择 JSON 文件，校验并保存到主题库；不会自动启用。
- 保存主题：保存库记录。修改正在使用的主题时，公开站点仍保持已启用快照。
- 启用主题：保存后点击“启用主题”，复制当前包为线上快照；刷新公开页面即可看到。
- 导出主题包：下载当前编辑内容（包含尚未保存的修改），用于分享或备份。
- 恢复默认：切换到内置主题，保留主题库。启用中的主题须先切换才能删除。

版本冲突时输入保留；导出当前内容，再重新打开主题页读取最新记录。
离开未保存的编辑会提示。导入文件不超过 100 KB，自定义 CSS 不超过 65,536 字节。

## 页面布局与 CSS

颜色提供 violet / teal / blue / rose 四个 Material 配色，字体提供 sans / serif / mono 系统字体族。
内容宽度 640–1600px，文章卡片 1–3 列或单列列表。侧边导航只在宽屏显示；移动端使用顶部导航。
首页区块用逗号分隔：hero（引言）、posts（文章，必需）、pages（已发布独立页面目录）。
顺序可以调整，可移除引言或目录；区块不能重复。

CSS 可使用 `.hero`、`.post-list`、`.post-card`、`.prose`、`header.site`、`footer.site`，
或覆盖 `--reading-width`、`--surface`、`--radius` 等变量。
管理员需要自行保持自定义样式的对比度、响应式布局和可访问性。
主题只影响公开站点，登录、setup 与后台保持可访问的内置样式。
右上角外观按钮可依次选择跟随系统、浅色、深色，保存在当前浏览器中。

预览禁用脚本、表单和外部资源；因此外部 CSS/图片在预览中不会加载，线上可按管理员 CSS 引用。
文本作为文本转义，CSS 的 HTML 闭合符号转义。主题不是执行不可信代码的插件沙箱。

## 格式与持久化

示例包见 [material-teal.json](themes/material-teal.json)。`schema_version` 固定为 1；未知字段拒绝。
接口见 [OpenAPI](api/openapi.json)：GET /api/v1/theme 提供公开快照；
/api/v1/admin/themes 提供库操作；PUT /api/v1/admin/themes/active 启用或恢复默认。
写操作需要管理员会话、CSRF 和更新版本。启用时同时提交 theme_version（已预览的库版本）与 version（启用状态版本），过期任一版本返回 409。

第六条 expand 迁移新增 themes 与 theme_settings，不修改旧表；现有部署先执行 `sloray migrate up`。
主题与启用快照在 Postgres，备份包含全部记录，不在磁盘写入主题文件或修改 config.json。
启用/恢复在同一事务内生成 CDN purge outbox，worker 消费；文章 HTML 的 ETag 包含最终主题样式。
