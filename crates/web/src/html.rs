//! 极简 HTML 渲染，只服务于 setup / 登录 / 设置这三类页面。
//!
//! 刻意**不引入模板引擎**：这些页面的动态内容很少，
//! 而"少一个依赖"在部署到小 VPS 的场景下是实打实的好处（也更少一处供应链风险）。
//!
//! 代价是必须手动转义，所以约定：
//! **一切进入 HTML 的动态值都必须经过 [`escape`]**，没有例外。
//! 这条约定由模块末尾的测试守护。
//!
//! 为什么不用 Astro 渲染这几页：setup 发生在"还没有数据库、也可能还没配
//! Nginx/域名/TLS"的时刻，此时唯一确定可用的进程就是 API 自己。
//! 把它做成零依赖的自包含页面，才谈得上"打开 http://<ip>:8080/setup 就能配"。

/// 把任意字符串安全地放进 HTML 文本节点或双引号属性值里。
///
/// 单引号也一并转义：很多属性坑（例如某天有人写成 `attr='...'`）
/// 都是因为只转义了双引号。
pub fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 提示条。`kind` 只允许 "ok" / "warn" / "err"，其余一律当 err 处理，
/// 避免调用方把用户输入当 class 名塞进来。
pub fn notice(kind: &str, message: &str) -> String {
    let kind = match kind {
        "ok" | "warn" | "err" => kind,
        _ => "err",
    };
    format!(r#"<p class="notice {kind}">{}</p>"#, escape(message))
}

/// 页面外壳。样式内联：setup 阶段可能还没有任何静态资源路由，
/// 页面必须自己就能看。
pub fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<title>{title}</title>
<style>{css}
{toast_css}
.skip-link {{ position: absolute; left: -10000px; }} .skip-link:focus {{ left: 1rem; }}
main {{ width: min(100%, 760px); margin: 2rem auto; padding: 1.5rem; }}
.settings-dock {{ position: fixed; z-index: 10; left: max(24px,calc((100vw - 1440px)/2)); top: 2rem; width: 76px; padding: 9px; background: var(--surface-container); border: 1px solid var(--outline); border-radius: 40px; box-shadow: 0 8px 32px rgb(0 0 0 / .16); }}
.settings-dock summary {{ display: none; cursor: pointer; padding: .75rem; min-height: 48px; }}
.settings-nav {{ display: flex; flex-direction: column; gap: 8px; }}
.settings-nav a {{ min-height: 52px; height: 52px; width: 52px; display: grid; place-items: center; padding: 0; border-radius: 50%; background: var(--surface-high); text-decoration: none; color: var(--fg-muted); }}
.settings-nav a[aria-current] {{ background: var(--on-surface); color: var(--surface); box-shadow: 0 0 24px rgb(255 255 255 / .12); }}
body:has(.settings-dock) {{ padding-left: 112px; }}
@supports (backdrop-filter: blur(20px)) {{ .settings-dock {{ background: color-mix(in srgb,var(--surface-container) 94%,transparent); backdrop-filter: blur(24px); }} }}
@media (max-width: 999px) {{ body:has(.settings-dock) {{ padding-left: 0; padding-bottom: 12rem; }} .settings-dock {{ top: auto; bottom: calc(.75rem + env(safe-area-inset-bottom)); left: 50%; transform: translateX(-50%); width: min(350px,calc(100% - 1.5rem)); max-height: 60dvh; overflow: auto; }} .settings-dock summary {{ display: grid; place-items: center; }} .settings-nav {{ flex-direction: row; flex-wrap: wrap; justify-content: center; }} .settings-nav a {{ width: 48px; height: 48px; min-height: 48px; }} }}
footer {{ margin-top: 2rem; color: var(--fg-muted); font-size: .85rem; }}
</style>
</head>
<body>{icons}<a class="skip-link" href='#content'>跳到内容</a><main id="content">
{body}
<footer>sloray-blog · <code>{version}</code></footer>
</main><script>{toast_script}</script></body></html>"#,
        css = include_str!("../../../web/src/styles/material.css"),
        icons = include_str!("../../../web/src/assets/material-icons.svg"),
        toast_css = include_str!("../../../web/src/styles/toasts.css"),
        toast_script = include_str!("../../../web/src/lib/toast-runtime.js"),
        title = escape(title),
        body = body,
        version = env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_five_dangerous_characters() {
        assert_eq!(
            escape(r#"<script>"x" & 'y'</script>"#),
            "&lt;script&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn escape_is_not_double_applied_to_plain_text() {
        assert_eq!(escape("普通文本 abc 123"), "普通文本 abc 123");
    }

    #[test]
    fn attribute_injection_is_neutralised() {
        // 这是最典型的一类攻击：把值塞进 value="..." 时闭合引号。
        let evil = r#"" autofocus onfocus="alert(1)"#;
        let rendered = escape(evil);
        assert!(!rendered.contains('"'), "转义后不应再出现裸双引号");
        assert!(rendered.contains("&quot;"));
    }

    #[test]
    fn notice_rejects_arbitrary_class_names() {
        // 调用方不能借 kind 注入 class 或标签。
        let rendered = notice(r#"x" onclick="evil()"#, "hi");
        assert!(rendered.contains(r#"class="notice err""#));
    }
}
