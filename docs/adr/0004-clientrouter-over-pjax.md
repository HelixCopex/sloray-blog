# ADR-0004：局部导航用 Astro 内置 ClientRouter，不手写 pjax

**状态**：已接受

## 背景

需求里写的是「Astro (pjax/ajax)」—— 希望页面之间的跳转不要整页刷新。
可选方案：

1. 手写 pjax：拦截 `<a>` 点击 → `fetch` → 替换容器 HTML → `pushState`；
2. 引入第三方 pjax 库（Swup、Turbo、barba.js）；
3. 用 Astro 内置的 `<ClientRouter />`（基于浏览器 View Transitions API）。

## 决策

采用方案 3。在 `web/src/layouts/BaseLayout.astro` 的 `<head>` 里挂一次：

```astro
import { ClientRouter } from 'astro:transitions';
<ClientRouter />
```

**不要**同时使用方案 1 或 2。

## 理由

手写 pjax 的"简单版本"确实只要几十行，但生产可用的版本必须处理：

| 问题 | 说明 |
|---|---|
| 脚本重新执行 | 换页后新内容里的脚本不会自动运行，需要手动处理 |
| 监听器重复绑定 | 若在换页后重新绑定而未清理，事件会触发多次 |
| `<head>` 同步 | `<title>`、meta、canonical 必须跟着更新，否则 SEO 与分享卡片错乱 |
| 滚动位置 | 前进/后退要恢复位置，新页面要滚到顶部 |
| 焦点与无障碍 | 换页后焦点必须移动并播报，否则键盘与读屏用户会迷失 |
| 中止竞态 | 快速点击多个链接时，后到的响应可能覆盖先到的 |
| 表单与 history | 带状态的页面回退时行为正确 |

`ClientRouter` 已覆盖以上全部，且是框架一等公民。第三方 pjax 库与它**会争抢
history 和滚动**，所以是互斥的，不是叠加关系。

## 影响

* **必须监听 `astro:page-load`**，不能只监听 `DOMContentLoaded`。
  这是本决策最重要的实践后果 —— 见 `web/src/components/NavStatus.astro`
  里的注释：绑定类代码要在 `astro:page-load` 上做。
* 需要跨页面保持状态的组件用 `transition:persist`
  （`#nav-status` 就是例子，它的计时器跨导航持续走动）。
* 共享元素动画用 `transition:name`。
* 预取默认开启（`prefetchAll`）。**必须排除 `/admin/*`**：
  预取会真的发起请求，可能触发鉴权检查或产生副作用。
  在管理后台的链接上加 `data-astro-prefetch="false"`。
* Astro 的 CSP 特性与 `<ClientRouter />` 不兼容（见官方配置文档），
  将来上严格 CSP 时需要在两者间取舍。
