#!/usr/bin/env python3
"""验证文章 API、鉴权、并发版本、SSR、XSS、SEO 与缓存撤回。"""
import json
import uuid
from smoke_support import App

with App() as app:
    article = {"slug": "测试文章", "title": "文章 & 测试", "summary": "安全的摘要",
               "body_md": '# 正文\n\n**加粗**\n\n<script>window.pwned=true</script>\n<img src="x" onerror="alert(1)">\n\n[危险](javascript:alert(1))',
               "status": "draft", "tags": ["rust", "博客"], "lang": "zh"}
    assert app.json("/api/v1/admin/posts", "POST", article, cookie=False)[0] == 401
    assert app.json("/api/v1/admin/posts", "POST", article, csrf=False)[0] == 403
    status, _, post = app.json("/api/v1/admin/posts", "POST", article)
    assert status == 201, (status, post)
    # extractor 产生的拒绝也应遵守标准信封，不回显内部解析文本。
    bad_status,_,bad_body=app.request('/api/v1/admin/posts',method='POST',cookie=app.cookie,csrf=app.csrf,headers={'Content-Type':'application/json'})
    assert bad_status==400 and json.loads(bad_body)['error']['code']=='bad_request'
    assert app.json('/api/v1/admin/posts/not-a-uuid')[2]['error']['code']=='bad_request'
    identifier = post["id"]
    assert app.json("/api/v1/posts/测试文章", cookie=False)[0] == 404
    article |= {"status": "published", "version": 1}
    assert app.json(f"/api/v1/admin/posts/{identifier}", "PUT", article)[0] == 200
    assert app.json(f"/api/v1/admin/posts/{identifier}", "PUT", article)[0] == 409
    assert len(app.json(f"/api/v1/admin/posts/{identifier}/revisions")[2]["items"]) == 2
    assert app.json("/api/v1/posts?tag=rust", cookie=False)[2]["items"][0]["id"] == identifier
    assert {t["slug"] for t in app.json("/api/v1/tags", cookie=False)[2]} == {"rust", "博客"}
    print("PASS 文章创建/发布/标签/修订，未登录和缺少 CSRF 拒绝写入，旧版本更新返回 409")
    status, headers, _ = app.json("/api/v1/posts/测试文章", cookie=False)
    assert status == 200
    api_etag = headers["etag"]
    assert app.request("/api/v1/posts/测试文章", headers={"If-None-Match": headers["etag"]})[0] == 304
    # 作者只能编辑自己的文章，公开 DTO 不含密码哈希。
    created = app.json("/api/v1/admin/users", "POST", {"email": "author@example.com", "display_name": "作者", "password": "a-test-only-password", "role": "author"})
    assert created[0] == 201 and "password_hash" not in created[2]
    status, headers, _ = app.request("/login", cookie=app.cookie, form={"email": "author@example.com", "password": "a-test-only-password", "csrf_token": app.csrf})
    assert status == 303
    author_cookie = headers["set-cookie"].split(";", 1)[0]
    assert app.request(f"/api/v1/admin/posts/{identifier}", cookie=author_cookie)[0] == 403
    print("PASS 会话与作者权限，公共详情支持 ETag/304")
    app.start_web()
    status, headers, html = app.request("/posts/%E6%B5%8B%E8%AF%95%E6%96%87%E7%AB%A0", web=True)
    assert status == 200, status
    assert b'<strong>' in html and b'window.pwned' not in html and b'onerror=' not in html and b'javascript:' not in html
    assert b'application/ld+json' in html and b'BlogPosting' in html
    assert app.request("/posts/%E6%B5%8B%E8%AF%95%E6%96%87%E7%AB%A0", headers={"If-None-Match": headers["etag"]}, web=True)[0] == 304
    for path in ("/rss.xml", "/sitemap.xml"):
        status, _, body = app.request(path, web=True)
        assert status == 200
        assert identifier.encode() in body if path == "/rss.xml" else b'%E6%B5%8B' in body
    assert app.request("/admin/posts", web=True)[0] == 302
    assert app.request("/admin/posts", cookie=app.cookie, web=True)[0] == 200
    print("PASS 无 node_modules 的 SSR 发布包、内网 API 故障切换、Markdown 清洗、SEO、后台登录和 HTML ETag")
    article["status"], article["version"] = "draft", 2
    assert app.json(f"/api/v1/admin/posts/{identifier}", "PUT", article)[0] == 200
    assert app.request("/api/v1/posts/测试文章", headers={"If-None-Match": api_etag})[0] == 404
    assert app.request("/posts/%E6%B5%8B%E8%AF%95%E6%96%87%E7%AB%A0", headers={"If-None-Match": headers["etag"]}, web=True)[0] == 404
    assert app.json(f"/api/v1/admin/posts/{identifier}", "DELETE", {"version": 3})[0] == 204
    assert app.json(f"/api/v1/admin/posts/{identifier}")[0] == 404
    print("PASS 撤回后旧 ETag 不会返回 304；软删除后后台和公开接口不可见")
