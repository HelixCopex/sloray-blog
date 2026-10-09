"""端到端测试的临时数据库、API 进程与 HTTP 客户端。"""
import http.client
import json
import os
import pathlib
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.parse
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class App:
    def __enter__(self):
        try:
            return self._start()
        except BaseException:
            if hasattr(self, "processes"):
                self.__exit__()
            raise

    def _start(self):
        self.base = os.environ["SLORAY_TEST_DATABASE_URL"]
        self.name = "sloray_content_" + uuid.uuid4().hex
        parts = urllib.parse.urlsplit(self.base)
        self.url = urllib.parse.urlunsplit(parts._replace(path="/" + self.name))
        self.sql(f'CREATE DATABASE "{self.name}" TEMPLATE template0 ENCODING \'UTF8\'', self.base)
        self.tmp = tempfile.TemporaryDirectory(prefix="sloray-content-")
        self.root = pathlib.Path(self.tmp.name)
        self.processes, self.logs = [], []
        self.api_port = port()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("SLORAY_")}
        self.env.update({"SLORAY_CONFIG_FILE": str(self.root / "config.json")})
        binary = ROOT / "target/debug/sloray"
        token = subprocess.check_output([str(binary), "setup-token"], env=self.env, text=True, stderr=subprocess.DEVNULL).strip()
        self.spawn([str(binary), "serve", "--bind", f"127.0.0.1:{self.api_port}"], self.env)
        self.wait(lambda: self.request("/healthz")[0] == 200)
        status, headers, _ = self.request("/setup", form={"token": token, "db_url": self.url,
            "admin_email": "admin@example.com", "admin_name": "测试站长",
            "password": "a-test-only-password", "password2": "a-test-only-password", "site_url": "http://localhost:4321"})
        assert status == 303, status
        self.cookie = headers["set-cookie"].split(";", 1)[0]
        _, headers, body = self.request("/api/v1/auth/session", cookie=self.cookie)
        self.csrf = json.loads(body)["csrf_token"]
        self.cookie += "; " + headers["set-cookie"].split(";", 1)[0]
        return self

    def sql(self, statement, url=None):
        result = subprocess.run(["psql", "--dbname", url or self.url, "-v", "ON_ERROR_STOP=1", "-tAc", statement], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError("隔离测试数据库命令失败")
        return result.stdout.strip()

    def spawn(self, args, env):
        log = open(self.root / f"process-{len(self.logs)}.log", "w+")
        self.logs.append(log)
        process = subprocess.Popen(args, env=env, stdout=log, stderr=log)
        self.processes.append(process)
        return process

    def wait(self, check):
        for _ in range(150):
            try:
                if check():
                    return
            except (OSError, http.client.HTTPException):
                pass
            if any(p.poll() is not None for p in self.processes):
                for log in self.logs:
                    log.flush()
                    log.seek(0)
                    # 不回显含 setup token 的日志。
                raise RuntimeError("测试进程提前退出；检查渲染包/配置")
            time.sleep(0.1)
        raise AssertionError("等待测试服务超时")

    def request(self, path, method="GET", body=None, cookie=None, form=None, csrf=None, headers=None, web=False):
        conn = http.client.HTTPConnection("127.0.0.1", self.web_port if web else self.api_port, timeout=10)
        hdrs = dict(headers or {})
        if cookie:
            hdrs["Cookie"] = cookie
        if csrf:
            hdrs["X-CSRF-Token"] = csrf
        data = None
        if form is not None:
            method, data = "POST", urllib.parse.urlencode(form)
            hdrs["Content-Type"] = "application/x-www-form-urlencoded"
        elif body is not None:
            data = json.dumps(body)
            hdrs["Content-Type"] = "application/json"
        conn.request(method, urllib.parse.quote(path, safe="/%?=&+"), data, hdrs)
        response = conn.getresponse()
        result = response.status, {k.lower(): v for k, v in response.getheaders()}, response.read()
        conn.close()
        return result

    def json(self, path, method="GET", body=None, cookie=True, csrf=True):
        status, headers, raw = self.request(path, method, body, self.cookie if cookie else None, csrf=self.csrf if csrf else None)
        return status, headers, json.loads(raw) if raw else None

    def start_web(self):
        self.web_port = port()
        # 复制到无 node_modules 的发布目录，防止构建机依赖掩盖遗漏。
        shutil.copytree(ROOT / "web/dist", self.root / "dist")
        env = self.env | {"HOST": "127.0.0.1", "PORT": str(self.web_port), "SLORAY_API_INTERNAL_URLS": f"http://127.0.0.1:1,http://127.0.0.1:{self.api_port}"}
        node = os.environ.get("NODE_BIN", "node")
        self.spawn([node, str(self.root / "dist/server/entry.mjs")], env)
        self.wait(lambda: self.request("/about", web=True)[0] == 200)

    def __exit__(self, *_args):
        for process in self.processes:
            process.terminate()
        for process in self.processes:
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for log in self.logs:
            log.close()
        self.sql(f'DROP DATABASE "{self.name}" WITH (FORCE)', self.base)
        self.tmp.cleanup()
