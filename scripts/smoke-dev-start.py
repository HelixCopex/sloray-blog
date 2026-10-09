#!/usr/bin/env python3
"""真实 API / Astro / worker：统一启动后消费媒体任务并生成可访问的 WebP。"""
import hashlib
import importlib.util
import json
import signal
import socket
import struct
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
    app.api_port, app.web_port = port(), port()
    env = app.env | {"SLORAY_DEV_API_PORT": str(app.api_port), "SLORAY_DEV_WEB_PORT": str(app.web_port)}
    process = app.spawn([str(ROOT / "scripts/dev-start.sh")], env)
    app.wait(lambda: app.request("/about", web=True)[0] == 200)
    assert app.request("/readyz", web=True)[0] == 200
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
