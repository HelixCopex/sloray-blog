#!/usr/bin/env python3
"""真实 API / Astro / worker：升级旧库、主题群岛、重复启动及媒体派生。"""
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import signal
import socket
import struct
import subprocess
import time
import urllib.request
import zlib
from smoke_support import App, ROOT, port

spec = importlib.util.spec_from_file_location("fake_objects", ROOT / "scripts/fake-objects.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def png():
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", 400, 200, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress((b"\0" + b"\x33\x66\x99" * 400) * 200))
            + chunk(b"IEND", b""))


with module.Objects() as objects, App() as app:
    config_path = app.root / "config.json"
    config = json.loads(config_path.read_text())
    config |= {"s3": {"endpoint": objects.url, "bucket": "test", "region": "auto",
                      "access_key_id": "test-key", "secret_access_key": "test-secret"},
               "media_public_bucket": "test-public", "media_public_base_url": objects.url + "/test-public"}
    config_path.write_text(json.dumps(config))
    # 只降级一次性测试库，模拟尚未包含页面和主题的旧开发环境。
    status, _, post = app.json("/api/v1/admin/posts", "POST", {
        "slug": "launch-upgrade", "title": "开发升级保留", "body_md": "**保留正文**", "status": "published"})
    assert status == 201
    previous_api = app.processes.pop()
    previous_api.terminate()
    previous_api.wait(timeout=12)
    subprocess.run([str(ROOT / "target/debug/sloray"), "migrate", "down", "2"],
                   env=app.env, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert app.sql("SELECT count(*) FROM seaql_migrations") == "4"
    assert app.sql("SELECT to_regclass('pages') IS NULL AND to_regclass('themes') IS NULL") == "t"
    app.api_port, app.web_port = port(), port()
    env = app.env | {"SLORAY_DEV_API_PORT": str(app.api_port), "SLORAY_DEV_WEB_PORT": str(app.web_port)}
    process = app.spawn([str(ROOT / "scripts/dev-start.sh")], env)
    app.wait(lambda: "worker 已持续运行" in Path(app.logs[-1].name).read_text())
    app.wait(lambda: app.request("/about", web=True)[0] == 200)
    assert app.request("/readyz", web=True)[0] == 200
    assert app.sql("SELECT count(*) FROM seaql_migrations") == "6"
    assert app.sql("SELECT count(*) FROM pg_tables WHERE schemaname='public'") == "20"
    assert app.request("/posts/launch-upgrade", web=True)[0] == 200
    assert app.json(f"/api/v1/admin/posts/{post['id']}")[2]["body_md"] == "**保留正文**"
    status, _, state = app.json("/api/v1/theme", cookie=False)
    assert status == 200 and state["version"] == 1
    package = state["package"]
    package["name"] = "开发启动主题"
    package["palette"] = "teal"
    package["layout"]["home_blocks"] = ["hero", "pages", "posts"]
    package["layout"]["hero_title"] = "升级后的首页"
    package["css"] = ".hero {border: 2px solid teal;}"
    status, _, theme = app.json("/api/v1/admin/themes", "POST", {"package": package})
    assert status == 201
    assert app.json("/api/v1/admin/themes/active", "PUT", {
        "theme_id": theme["id"], "theme_version": theme["version"], "version": state["version"]})[0] == 200
    html = app.request("/", web=True)[2].decode()
    assert "升级后的首页" in html and ".hero {border: 2px solid teal;}" in html
    assert "/_server-islands/PageDirectory" in html
    status, _, html = app.request("/admin/themes", web=True, cookie=app.cookie)
    assert status == 200
    island = re.search(r'/_server-islands/ThemeWorkbench\?[^"< ]+', html.decode()).group(0).replace("&amp;", "&")
    status, _, html = app.request(island, web=True, cookie=app.cookie)
    assert status == 200 and "开发启动主题" in html.decode()
    print("PASS 自动升级四迁移旧库到六迁移/20 表、保留原有文章、页面与主题 API/SSR/群岛可用")
    body = png()
    status, _, signed = app.json("/api/v1/admin/media/presign", "POST", {
        "content_type": "image/png", "size_bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()})
    assert status == 200
    request = urllib.request.Request(signed["upload_url"], data=body, headers=signed["headers"], method="PUT")
    assert urllib.request.urlopen(request).status == 200
    status, _, media = app.json("/api/v1/admin/media", "POST", {"upload_id": signed["upload_id"]})
    assert status == 201
    app.wait(lambda: app.json(f"/api/v1/admin/media/{media['id']}")[2]["ready"])
    result = app.json(f"/api/v1/admin/media/{media['id']}")[2]
    image = urllib.request.urlopen(result["variants"][0]["url"]).read()
    assert image[:4] == b"RIFF" and image[8:12] == b"WEBP"
    assert app.sql("SELECT count(*) FROM outbox WHERE topic='media.generate' AND processed_at IS NOT NULL") == "1"
    print("PASS 统一启动真实 API/Astro/worker、同源代理、自动消费媒体任务及公开 WebP 可访问")
    process.send_signal(signal.SIGINT)
    assert process.wait(timeout=12) == 130
    # App.wait 不用于清理阶段，因为监督进程已经按预期退出。
    for value in (app.api_port, app.web_port):
        deadline = time.monotonic() + 5
        while True:
            with socket.socket() as listener:
                listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                try:
                    listener.bind(("127.0.0.1", value))
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise AssertionError("停止后开发服务仍占用端口") from None
            time.sleep(0.1)
    print("PASS 统一启动器 Ctrl+C 停止真实服务，API / Astro 端口均释放")
    app.processes.remove(process)
    process = app.spawn([str(ROOT / "scripts/dev-start.sh")], env)
    app.wait(lambda: "worker 已持续运行" in Path(app.logs[-1].name).read_text())
    app.wait(lambda: app.request("/posts/launch-upgrade", web=True)[0] == 200)
    assert app.sql("SELECT count(*) FROM seaql_migrations") == "6"
    assert app.sql("SELECT count(*) FROM pages WHERE slug='about'") == "1"
    assert app.json("/api/v1/theme", cookie=False)[2]["package"] == package
    assert app.json(f"/api/v1/admin/posts/{post['id']}")[2]["body_md"] == "**保留正文**"
    process.send_signal(signal.SIGINT)
    assert process.wait(timeout=12) == 130
    print("PASS 已升级开发库重复启动幂等，不重建页面、不覆盖已启用主题或文章")
