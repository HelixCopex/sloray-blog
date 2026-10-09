#!/usr/bin/env python3
"""为每次运行创建独立数据库，验证真实 CLI、setup 与两个 API 实例共享配置。"""
import concurrent.futures
import http.client
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.parse
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def main():
    base_url = os.environ.get("SLORAY_TEST_DATABASE_URL")
    if not base_url:
        raise SystemExit("必须提供 SLORAY_TEST_DATABASE_URL（仅使用专用测试实例）")
    binary = pathlib.Path(os.environ.get("SLORAY_BIN", ROOT / "target/debug/sloray")).resolve()
    name = "sloray_smoke_" + uuid.uuid4().hex
    parts = urllib.parse.urlsplit(base_url)
    db_url = urllib.parse.urlunsplit(parts._replace(path="/" + name))

    def sql(statement, url=base_url):
        result = subprocess.run(["psql", "--dbname", url, "-v", "ON_ERROR_STOP=1", "-tAc", statement], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError("Postgres 测试命令失败，请检查测试实例")
        return result.stdout.strip()

    sql(f'CREATE DATABASE "{name}" TEMPLATE template0 ENCODING \'UTF8\'')
    processes = []
    logs = []
    try:
        with tempfile.TemporaryDirectory(prefix="sloray-setup-") as tmp:
            config = pathlib.Path(tmp) / "config.json"
            env = {k: v for k, v in os.environ.items() if not k.startswith("SLORAY_")}
            env.update({"SLORAY_S3_ENDPOINT": "https://r2.example.com", "SLORAY_S3_REGION": "auto",
                        "SLORAY_S3_BUCKET": "test", "SLORAY_S3_ACCESS_KEY_ID": "test-key",
                        "SLORAY_S3_SECRET_ACCESS_KEY": "test-secret"})
            # 即使配置损坏，运维修复命令仍须可用；并发首次生成不能互相轮换。
            config.write_text("invalid JSON")
            def token(_index):
                return subprocess.check_output([str(binary), "--config", str(config), "setup-token"], env=env, stderr=subprocess.DEVNULL, text=True).strip()
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                tokens = list(pool.map(token, range(16)))
            assert len(set(tokens)) == 1, "并发 token 创建产生多个值"
            assert (pathlib.Path(tmp) / "setup-token").stat().st_mode & 0o777 == 0o600
            config.unlink()
            print("PASS 损坏配置下 setup-token 可用；16 个并发创建共享一个私有 token")

            ports = [free_port(), free_port()]
            for index, port in enumerate(ports):
                run_env = env.copy()
                if index == 1:
                    run_env["SLORAY_DATABASE_URL"] = db_url
                # EnvironmentFile 的默认端口不得压过实例的 --bind 参数。
                run_env["SLORAY_BIND_ADDR"] = "127.0.0.1:1"
                log = open(pathlib.Path(tmp) / f"server-{index}.log", "w+")
                logs.append(log)
                processes.append(subprocess.Popen([str(binary), "--config", str(config), "serve", "--bind", f"127.0.0.1:{port}"], env=run_env, stdout=log, stderr=log))

            def request(port, path, form=None, cookie=None):
                conn = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
                headers = {}
                if cookie:
                    headers["Cookie"] = cookie
                data = None
                if form is not None:
                    data = urllib.parse.urlencode(form)
                    headers["Content-Type"] = "application/x-www-form-urlencoded"
                conn.request("POST" if form is not None else "GET", path, data, headers)
                response = conn.getresponse()
                result = response.status, {key.lower(): value for key, value in response.getheaders()}, response.read()
                conn.close()
                return result

            def until(check):
                for _ in range(100):
                    try:
                        if check():
                            return
                    except (OSError, http.client.HTTPException):
                        pass
                    if any(process.poll() is not None for process in processes):
                        raise RuntimeError("测试服务提前退出")
                    time.sleep(0.1)
                raise AssertionError("等待测试服务状态超时")

            for port in ports:
                until(lambda: request(port, "/healthz")[0] == 200)
                status, headers, _ = request(port, "/setup")
                assert status == 200 and headers["cache-control"] == "no-store"
            print("PASS 仅配置连接串但无管理员时 setup 可用；两个实例使用各自 --bind 端口")
            form = {"token": tokens[0], "db_url": db_url, "admin_email": "admin@example.com",
                    "admin_name": "站长", "password": "a-test-only-password", "password2": "a-test-only-password",
                    "site_url": "http://localhost:4321"}
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                responses = list(pool.map(lambda port: request(port, "/setup", form), ports))
            assert all(response[0] == 303 for response in responses), [r[0] for r in responses]
            assert sql("SELECT count(*) FROM users WHERE role='admin'", db_url) == "1"
            for port in ports:
                until(lambda: request(port, "/readyz")[0] == 200)
                until(lambda: request(port, "/setup")[0] == 303)
            print("PASS 并发 setup 只创建一位管理员；共享配置后两实例自动就绪并关闭 setup")
            cookie = next(response[1]["set-cookie"].split(";", 1)[0] for response in responses if "set-cookie" in response[1])
            until(lambda: b'https://r2.example.com' in request(ports[0], "/admin/settings", cookie=cookie)[2])
            _, csrf_headers, csrf_raw = request(ports[0], "/api/v1/auth/session", cookie=cookie)
            csrf_token = json.loads(csrf_raw)["csrf_token"]
            cookie += "; " + csrf_headers["set-cookie"].split(";", 1)[0]
            settings = {"csrf_token": csrf_token, "site_url": "http://localhost:4321", "s3_endpoint": "https://r2.example.com",
                        "s3_region": "auto", "s3_bucket": "test", "s3_access_key_id": "test-key",
                        "s3_secret_access_key": "test-secret"}
            assert request(ports[0], "/admin/settings", settings, cookie)[0] == 200
            persisted = json.loads(config.read_text())
            assert persisted["s3"]["secret_access_key"] == "test-secret"
            assert config.stat().st_mode & 0o777 == 0o600
            assert request(ports[1], "/admin/settings", cookie=cookie)[0] == 200
            print("PASS 登录会话跨实例可用；配置以 0600 写入，扁平 S3 环境变量正确映射")
    finally:
        for process in processes:
            process.terminate()
        for process in processes:
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for log in logs:
            log.close()
        sql(f'DROP DATABASE "{name}" WITH (FORCE)')


if __name__ == "__main__":
    main()
