# ADR-0026：正文串行滑移动效

- 状态：已接受
- 日期：2026-10-10
- Supersedes ADR-0024 第 1 项的快照动效及 100ms 重叠交接，以及 ADR-0025 第 1 项的正文快照布局处理。

维护者在公开页和后台切页中看到旧、新文字重叠，主题页仍在 Zen 中出现压缩。
上一实现没有显式保持动画起止状态；新快照在延迟期间显示，旧快照退出后恢复可见。
延迟加载的主题群岛还会在浏览器捕获新正文后改变高度，快照尺寸不能代表最终内容。
此前仅检查样式和动画曲线、未保证截取真正的中间帧，不足以证明渲染正确。

1. Astro ClientRouter 继续管理请求、history、滚动、文档替换、脚本及持久节点。
   使用公开 lifecycle hooks，在 loader 成功后让当前 main 以 200ms MD3 emphasized-accelerate
   滑出淡出；退出完成后才执行文档替换。新 main 从透明状态以 350ms emphasized-decelerate
   滑入淡入。使用 Web Animations API 的 both 填充模式，退出结束保持透明，进入前保持透明。
   前进和返回方向相反，不缩放；取消操作恢复当前可见内容。
2. before-swap 调用 ViewTransition.skipTransition 跳过浏览器的截图绘制，正文自身参与动画。
   这样每帧只绘制当前 DOM，动态群岛、表单、预览及图片保持真实布局，不依赖截图的初始比例。
   兼容路径使用 ClientRouter 的 swap fallback，也由同一正文动画控制，不另写路由器。
3. Dock 与后台 Tab 不参与正文动效，继续持久化。读取和导航失败保留当前内容；被后续导航
   中止时取消旧动效并恢复样式。减少动态效果时直接换页。临时透明样式只在动效期间存在，
   不改变正文固有样式或读取权限。
4. 浏览器回归须确实暂停目标动画并检查时间和 playState，对退出、中间交接、进入及结束
   分别验证可见内容；加入主题与账号互切、动态群岛及极端长短内容。截图应来自过渡中，
   不能以最终页面截图或 object-fit 名称代替渲染验证。

不增加依赖或数据库迁移。图片聚焦、新 Dock 动作、Toast 和后台持久 Tab 的其它决策继续适用。

相关公开 API：
[Astro 导航 lifecycle](https://docs.astro.build/en/guides/view-transitions/#lifecycle-events)、
[ViewTransition.skipTransition](https://developer.mozilla.org/en-US/docs/Web/API/ViewTransition/skipTransition)。
