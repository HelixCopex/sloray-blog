#!/usr/bin/env python3
"""用真实 Nginx + 三个隔离上游验证生产配置的分流、头覆盖与故障切换。"""
import contextlib
import http.client
import http.server
import os
import pathlib
import shutil
import socket
import ssl
import subprocess
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.reply()

    def do_POST(self):
        self.server.posts += 1
        self.reply()

    def do_HEAD(self):
        self.reply()

    def reply(self):
        code = 503 if self.server.fail or (self.command == "POST" and self.server.post_fail) else 200
        self.send_response(code)
        self.send_header("X-Backend", self.server.name)
        self.send_header("X-Observed-IP", self.headers.get("X-Real-IP", "missing"))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if self.command != "HEAD": self.wfile.write(self.server.name.encode())

    def log_message(self, *_args):
        pass


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def main():
    nginx = os.environ.get("NGINX_BIN") or shutil.which("nginx")
    if not nginx:
        raise SystemExit("需要 nginx；可用 NGINX_BIN 指向本地二进制")
    servers = []
    for name in ("api-a", "api-b", "web"):
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        server.name, server.fail, server.post_fail, server.posts = name, False, True, 0
        threading.Thread(target=server.serve_forever, daemon=True).start()
        servers.append(server)
    try:
        with tempfile.TemporaryDirectory(prefix="sloray-nginx-") as tmp:
            root = pathlib.Path(tmp)
            (root / "logs").mkdir()
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                            "-subj", "/CN=localhost", "-keyout", str(root / "key.pem"),
                            "-out", str(root / "cert.pem")], check=True, capture_output=True)
            port, http_port = free_port(), free_port()
            config = (ROOT / "deploy/nginx/sloray.conf").read_text()
            for old, server in zip((8080, 8081, 4321), servers):
                config = config.replace(f"127.0.0.1:{old}", f"127.0.0.1:{server.server_port}")
            config = config.replace("listen 80;", f"listen 127.0.0.1:{http_port};")
            config = config.replace("listen 443 ssl;", f"listen 127.0.0.1:{port} ssl;")
            config = config.replace("listen [::]:80;", "").replace("listen [::]:443 ssl;", "")
            config = config.replace("/etc/letsencrypt/live/blog.example.com/fullchain.pem", str(root / "cert.pem"))
            config = config.replace("/etc/letsencrypt/live/blog.example.com/privkey.pem", str(root / "key.pem"))
            top = f"daemon off; master_process off; pid {root}/nginx.pid; error_log stderr; events {{}} http {{ access_log off; client_body_temp_path {root}/body; proxy_temp_path {root}/proxy; {config} }}"
            (root / "nginx.conf").write_text(top)
            args = [nginx, "-p", f"{root}/", "-c", str(root / "nginx.conf")]
            subprocess.run(args + ["-t"], check=True)
            process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                def request(path, method="GET"):
                    conn = http.client.HTTPSConnection("127.0.0.1", port, context=ssl._create_unverified_context(), timeout=3)
                    conn.request(method, path, headers={"X-Real-IP": "198.51.100.99"})
                    response = conn.getresponse()
                    body = response.read()
                    headers = dict(response.getheaders())
                    conn.close()
                    return response.status, headers, body

                for _ in range(50):
                    try:
                        request("/about")
                        break
                    except OSError:
                        if process.poll() is not None:
                            raise RuntimeError(process.stderr.read().decode())
                        time.sleep(0.05)
                # 两个上游均可连接；POST 返回 503 必须只投递一次，不能到另一实例重放。
                assert request('/api/v1/admin/posts', 'POST')[0] == 503
                assert sum(server.posts for server in servers[:2]) == 1
                print('PASS POST 失败仅投递一次，未重放到另一 API')
                conn=http.client.HTTPConnection('127.0.0.1',http_port,timeout=3)
                conn.request('GET','/posts/test?q=1'); result=conn.getresponse()
                assert result.status==308 and result.getheader('Location').endswith('/posts/test?q=1')
                result.read(); conn.close()
                for path in ("/api/v1/ping", "/healthz", "/readyz", "/setup", "/login", "/logout", "/admin/settings"):
                    status, headers, _ = request(path)
                    assert status == 200 and headers["X-Backend"].startswith("api"), (path, status, headers)
                    assert headers["X-Observed-IP"] == "127.0.0.1", headers
                    assert headers["Cache-Control"] == "no-store", headers
                for path in ("/", "/about", "/admin/posts", "/admin/themes", "/_server-islands/ThemeWorkbench?e=test&p=&s=", "/_astro/test.js"):
                    assert request(path)[1]["X-Backend"] == "web", path
                print("PASS Nginx 配置、API/安装/登录/设置/SSR 分流、真实 IP 头覆盖")
                servers[0].fail = True
                for _ in range(4):
                    assert request("/api/v1/ping")[0] == 200
                    assert request("/api/v1/ping", "HEAD")[0] == 200
                print("PASS GET 在一个 API 返回 503 时切到健康上游")
                servers[1].fail = True
                assert request("/api/v1/posts", "POST")[0] == 503
                print("PASS 全部上游不可用时正确返回失败")
            finally:
                process.terminate()
                with contextlib.suppress(subprocess.TimeoutExpired):
                    process.communicate(timeout=5)
                if process.poll() is None:
                    process.kill()
                    process.communicate()
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    main()
