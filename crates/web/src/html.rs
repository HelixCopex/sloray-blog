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
<style>
:root {{ color-scheme: light dark; }}
* {{ box-sizing: border-box; }}
body {{
  margin: 0; padding: 2rem 1rem; line-height: 1.65;
  font-family: ui-sans-serif, system-ui, -apple-system, "Noto Sans SC", sans-serif;
  background: #fdfdfc; color: #1a1a1a;
}}
main {{ max-width: 40rem; margin: 0 auto; }}
h1 {{ font-size: 1.5rem; margin: 0 0 .25rem; }}
h2 {{ font-size: 1.05rem; margin: 1.75rem 0 .5rem; }}
p.sub {{ color: #5c5c5c; margin: 0 0 1.5rem; }}
label {{ display: block; margin: .9rem 0 .25rem; font-size: .9rem; font-weight: 600; }}
input, select {{
  width: 100%; padding: .5rem .6rem; font: inherit;
  border: 1px solid #d8d8d4; border-radius: 6px; background: #fff; color: inherit;
}}
.hint {{ color: #5c5c5c; font-size: .82rem; margin: .2rem 0 0; }}
button {{
  margin-top: 1.25rem; padding: .55rem 1.1rem; font: inherit; font-weight: 600;
  border: 0; border-radius: 6px; background: #2b6cb0; color: #fff; cursor: pointer;
}}
button.secondary {{ background: #e8e8e4; color: #1a1a1a; }}
fieldset {{ border: 1px solid #e3e3e0; border-radius: 8px; padding: 1rem 1.1rem .4rem; margin: 1.5rem 0 0; }}
legend {{ padding: 0 .4rem; font-size: .85rem; font-weight: 600; color: #5c5c5c; }}
.notice {{ padding: .6rem .8rem; border-radius: 6px; border: 1px solid; margin: 1rem 0; }}
.notice.ok {{ border-color: #2f855a; color: #2f855a; }}
.notice.warn {{ border-color: #b7791f; color: #b7791f; }}
.notice.err {{ border-color: #c53030; color: #c53030; }}
code {{ background: rgba(0,0,0,.06); padding: .1em .35em; border-radius: 4px; }}
footer {{ margin-top: 2.5rem; color: #5c5c5c; font-size: .8rem; }}
@media (prefers-color-scheme: dark) {{
  body {{ background: #16161a; color: #eaeaea; }}
  p.sub, .hint, footer, legend {{ color: #a0a0a0; }}
  input, select {{ background: #1e1e24; border-color: #2c2c33; }}
  button.secondary {{ background: #2c2c33; color: #eaeaea; }}
  fieldset {{ border-color: #2c2c33; }}
  code {{ background: rgba(255,255,255,.1); }}
}}
</style>
</head>
<body><main>
{body}
<footer>sloray-blog · <code>{version}</code></footer>
</main></body></html>"#,
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
