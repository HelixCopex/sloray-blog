#!/usr/bin/env python3
"""真实 API + Astro dev：覆盖启动、CommonJS、同源代理与 Markdown SSR。"""
import json
import os
from smoke_support import App, ROOT, port

with App() as app:
    status, _, post = app.json("/api/v1/admin/posts", "POST", {
        "slug": "dev-smoke", "title": "开发服务器测试", "status": "published",
        "body_md": "## 开发模式 Markdown\n\n**真实内容**\n\n<script>alert(1)</script>",
    })
    assert status == 201
    app.web_port = port()
    env = app.env | {"ASTRO_TELEMETRY_DISABLED": "1",
                     "SLORAY_API_INTERNAL_URLS": f"http://127.0.0.1:{app.api_port}"}
    astro = ROOT / "web/node_modules/astro"
    cli = json.loads((astro / "package.json").read_text())["bin"]["astro"]
    process = app.spawn([os.environ.get("NODE_BIN", "node"),
                         str(astro / cli), "dev",
                         "--root", str(ROOT / "web"), "--ignore-lock", "--port", str(app.web_port)], env)
    # 先请求静态页，不能只以端口打开作为启动成功的证据。
    app.wait(lambda: app.request("/about", web=True)[0] == 200)
    assert app.request("/healthz", web=True)[0] == 200
    assert app.request("/readyz", web=True)[0] == 200
    status, _, body = app.request("/api/v1/posts", web=True)
    assert status == 200 and "dev-smoke" in body.decode()
    status, _, body = app.request("/setup", web=True)
    assert status in (302, 303), status  # 完成初始化后关闭向导。
    status, headers, _ = app.request("/login", web=True)
    assert status == 200 and "set-cookie" in headers
    assert app.request("/admin/settings", web=True)[0] in (302, 303)
    status, _, body = app.request("/posts/dev-smoke", web=True)
    html = body.decode()
    assert status == 200 and "<strong>真实内容</strong>" in html
    assert "<script>alert(1)</script>" not in html
    version = post["version"]
    status, _, _ = app.json(f"/api/v1/admin/posts/{post['id']}", "PUT", {
        "slug": "dev-smoke", "title": "开发服务器测试", "status": "published",
        "body_md": "**修改即时可见**", "version": version,
    })
    assert status == 200
    assert "<strong>修改即时可见</strong>" in app.request("/posts/dev-smoke", web=True)[2].decode()
    assert process.poll() is None
    print("PASS Astro dev 实际页面、同源 API/登录/设置/探针代理、Markdown 清洗与内容即时更新")
