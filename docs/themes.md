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
内容宽度 640–1600px，文章卡片 1–3 列或单列列表。导航统一为桌面侧边栏与手机底部可展开 Dock；旧 navigation 选项兼容保留，均遵守该规则。
首页区块用逗号分隔：hero（引言）、posts（文章，必需）、pages（已发布独立页面目录）、widgets（默认小组件域）。
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

## 首页编排与独立刷新

主题工作台的“首页编排”显示纵向区块，可上移、下移、移除引言，或添加组件域。
文章列表必须保留且只能出现一次；最多一个引言、12 个区块，每个组件域最多 4 个小组件。
便笺可写标题和纯文字，标签从当前公开数据读取，页面组件列出已发布页面，RSS 提供订阅入口。
组件可向前调整顺序。没有组件的域自动移除。修改后先保存主题，再启用到全站。

导出包的 layout.home_rows 为可选数组，结构如下；旧包不含该字段时继续按 home_blocks 排列。

```json
[
  {"kind":"hero","title":"","widgets":[]},
  {"kind":"widgets","title":"随手探索","widgets":[
    {"kind":"note","title":"一则便笺","text":"留一点时间给阅读。"},
    {"kind":"tags","title":"标签","text":""}
  ]},
  {"kind":"posts","title":"文字档案","widgets":[]}
]
```

首次小组件加载采用 Astro server:defer；卡片右上角刷新只重新读取和渲染该卡片。
正文与文章列表不等待组件，也不会随组件刷新重载。读取失败可单独重试。
想恢复旧 home_blocks 布局，可以编辑“首页区块顺序”，该操作会清空新编排设置。

## 站点名称、Favicon 与阅读偏好

在 `/admin/settings` 的“站点标识”设置名称和 Favicon，使用独立保存按钮。
名称同步到站点导航、浏览器标题和 RSS；名称链接返回首页，导航不再提供单独“首页”按钮。
Favicon 填同源绝对路径或 HTTPS 图片地址，可从媒体库复制公开图片 URL；留空使用内置图标。
设置保存到 Postgres，带版本冲突提示；更新不会改动数据库或对象存储部署配置。
已有部署须先应用第七条迁移 `sloray migrate up`。

手机 Dock 展开显示所有导航，Escape 或点击外部收起；深浅外观与字号偏好仅保存在读者浏览器。
文章页提供字号、目录、阅读进度、专注阅读和代码复制；编辑器提供格式工具及实时字数，Ctrl/⌘+S 保存。
所有动效尊重系统“减少动态效果”偏好。浏览器不支持 backdrop-filter 时使用实色表面。
