#!/usr/bin/env python3
"""真实进程/HTTP 验证依赖同步、迁移顺序、共用配置及进程组清理。"""
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def wait(condition):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(0.1)
    raise AssertionError("启动器状态等待超时")


def alive(pid):
    try:
        # 被 PID 1 尚未回收的 zombie 已经退出，不再占有端口或执行任务。
        return Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1][0] != "Z"
    except FileNotFoundError:
        return False


MOCK = '''#!/usr/bin/env python3
import http.server, json, os, pathlib, subprocess, sys, time
root = pathlib.Path(os.environ['DEV_TEST_ROOT'])
mode = 'install' if 'install' in sys.argv else ('web' if '--port' in sys.argv else sys.argv[1])
(root / (mode + '.json')).write_text(json.dumps({
    'pid': os.getpid(), 'config': os.environ['SLORAY_CONFIG_FILE'], 'args': sys.argv[1:]}))
if mode == 'install':
    if os.environ.get('DEV_TEST_FAIL_INSTALL'): sys.exit(8)
    (root / 'web/node_modules').mkdir(exist_ok=True)
    sys.exit(0)
if mode == 'migrate':
    assert sys.argv[2:] == ['up'], '开发启动只能 migrate up'
    if os.environ.get('DEV_TEST_MIGRATION_CHILD'):
        subprocess.Popen([sys.executable, __file__, 'migration-child'])
    while os.environ.get('DEV_TEST_BLOCK_MIGRATION') and not (root / 'finish-migration').exists():
        time.sleep(0.1)
    if os.environ.get('DEV_TEST_FAIL_MIGRATION'): sys.exit(9)
    (root / 'migrated').touch()
    sys.exit(0)
if mode == 'worker': assert (root / 'migrated').exists(), '迁移成功前不能启动 worker'
if mode == 'worker' and os.environ.get('DEV_TEST_FAIL_WORKER'):
    sys.exit(7)
if mode in ('worker', 'grandchild', 'migration-child'):
    while True: time.sleep(1)
if mode == 'serve':
    subprocess.Popen([sys.executable, __file__, 'grandchild'])
    port = int(sys.argv[-1].split(':')[-1])
else:
    port = int(sys.argv[sys.argv.index('--port') + 1])
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(503 if self.path == '/readyz' and not (root / 'ready').exists() else 200)
        self.end_headers()
    def log_message(self, *args): pass
http.server.HTTPServer.allow_reuse_address = True
http.server.HTTPServer(('127.0.0.1', port), Handler).serve_forever()
'''


with tempfile.TemporaryDirectory(prefix="sloray dev start ") as directory:
    fixture = Path(directory)
    (fixture / "scripts").mkdir()
    (fixture / "web").mkdir()
    (fixture / "tools").mkdir()
    launcher = fixture / "scripts/dev-start.sh"
    shutil.copyfile(ROOT / "scripts/dev-start.sh", launcher)
    launcher.chmod(0o755)
    mock = fixture / "tools/mock"
    mock.write_text(MOCK)
    mock.chmod(0o755)
    cargo = fixture / "tools/cargo"
    cargo.write_text('''#!/usr/bin/env bash
set -eu
mkdir -p "$DEV_TEST_ROOT/target/debug"
cp "$DEV_TEST_ROOT/tools/mock" "$DEV_TEST_ROOT/target/debug/sloray"
''')
    cargo.chmod(0o755)
    shutil.copyfile(mock, fixture / "tools/pnpm")
    (fixture / "tools/pnpm").chmod(0o755)
    legacy = fixture / "web/.dev-cache/runtime/config.json"
    legacy.parent.mkdir(parents=True)
    legacy.write_text('{"secret":"must-not-be-printed"}')
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("SLORAY_") and key != "CARGO_TARGET_DIR"}
    env |= {"PATH": f"{fixture / 'tools'}:{env['PATH']}",
            "DEV_TEST_ROOT": str(fixture), "SLORAY_DEV_API_PORT": str(port()),
            "SLORAY_DEV_WEB_PORT": str(port())}

    def record(name):
        return json.loads((fixture / f"{name}.json").read_text())

    def run(extra=None, cwd=None):
        return subprocess.Popen([str(launcher)], cwd=cwd or fixture, env=env | (extra or {}),
                                stdout=output, stderr=output)

    def verify_cleanup():
        for name in ("serve", "web", "worker", "grandchild", "migrate", "migration-child"):
            if (fixture / f"{name}.json").exists():
                wait(lambda: not alive(record(name)["pid"]))

    def clear():
        for name in ("install", "serve", "web", "worker", "grandchild", "migrate", "migration-child"):
            (fixture / f"{name}.json").unlink(missing_ok=True)
        for name in ("migrated", "finish-migration"):
            (fixture / name).unlink(missing_ok=True)

    def stop(process):
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=12)

    def response_status(path):
        connection = http.client.HTTPConnection("127.0.0.1", int(env["SLORAY_DEV_WEB_PORT"]), timeout=2)
        connection.request("GET", path)
        response = connection.getresponse()
        response.read()
        connection.close()
        return response.status

    with open(fixture / "output.log", "w+") as output:
        process = run({"DEV_TEST_BLOCK_MIGRATION": "1"})
        try:
            wait(lambda: (fixture / "serve.json").exists() and (fixture / "web.json").exists()
                 and (fixture / "grandchild.json").exists())
            time.sleep(1.5)
            assert not (fixture / "worker.json").exists(), "未就绪不能启动 worker"
            assert not (fixture / "migrate.json").exists(), "数据库不可用时不尝试迁移"
            assert response_status("/setup") == 200 and response_status("/readyz") == 503
            assert (fixture / "web/node_modules").is_dir(), "首次启动自动安装依赖"
            assert record("install")["args"] == ["--dir", str(fixture / "web"), "install", "--frozen-lockfile"]
            (fixture / "ready").touch()
            wait(lambda: (fixture / "migrate.json").exists())
            time.sleep(1.5)
            assert not (fixture / "worker.json").exists(), "迁移尚未完成不能启动 worker"
            (fixture / "finish-migration").touch()
            wait(lambda: (fixture / "worker.json").exists())
            for name in ("install", "serve", "migrate", "worker"):
                assert record(name)["config"] == str(legacy)
            process.send_signal(signal.SIGINT)
            assert process.wait(timeout=12) == 130
            verify_cleanup()
            for log in (fixture / ".dev-cache/dev").glob("*.log"):
                assert log.stat().st_mode & 0o777 == 0o600
            print("PASS 首次依赖安装、旧 web 配置复用、等待数据库和迁移后启动 worker、私有日志及 Ctrl+C 清理")
        finally:
            stop(process)

        # 显式相对配置在 cd 前解析；worker 退出必须带走其它服务。
        clear()
        process = run({"SLORAY_CONFIG_FILE": ".dev-cache/runtime/config.json",
                       "DEV_TEST_FAIL_WORKER": "1"}, cwd=fixture / "web")
        try:
            assert process.wait(timeout=15) != 0
            assert record("worker")["config"] == str(legacy)
            verify_cleanup()
            output.flush()
            text = (fixture / "output.log").read_text()
            assert "worker 意外退出（状态 7）" in text
            assert "must-not-be-printed" not in text
            print("PASS 相对路径共用配置、worker 失败停止全部服务、未输出配置密钥")
        finally:
            stop(process)

        clear()
        process = run({"DEV_TEST_FAIL_MIGRATION": "1"})
        try:
            assert process.wait(timeout=15) != 0
            assert not (fixture / "worker.json").exists()
            verify_cleanup()
            output.flush()
            assert "数据库迁移失败（状态 9）" in (fixture / "output.log").read_text()
            print("PASS 迁移失败阻止 worker，停止 API/Astro 并提示迁移日志")
        finally:
            stop(process)

        clear()
        process = run({"DEV_TEST_BLOCK_MIGRATION": "1", "DEV_TEST_MIGRATION_CHILD": "1"})
        try:
            wait(lambda: (fixture / "migration-child.json").exists())
            process.send_signal(signal.SIGINT)
            assert process.wait(timeout=12) == 130
            assert not (fixture / "worker.json").exists()
            verify_cleanup()
            print("PASS Ctrl+C 中断迁移时清理迁移进程组及子进程")
        finally:
            stop(process)

        clear()
        process = run({"DEV_TEST_FAIL_INSTALL": "1"})
        assert process.wait(timeout=5) != 0
        assert not (fixture / "serve.json").exists() and not (fixture / "web.json").exists()
        print("PASS 锁定依赖安装失败时不启动任何业务服务")

        # 没有配置的新环境仍能使用安装向导，不执行无连接串的迁移。
        clear()
        legacy.unlink()
        (fixture / "ready").unlink()
        process = run()
        try:
            wait(lambda: (fixture / "grandchild.json").exists() and (fixture / "web.json").exists())
            time.sleep(1.5)
            assert response_status("/setup") == 200
            assert not (fixture / "migrate.json").exists() and not (fixture / "worker.json").exists()
            assert record("serve")["config"] == str(fixture / ".dev-cache/runtime/config.json")
            process.send_signal(signal.SIGTERM)
            assert process.wait(timeout=12) == 143
            verify_cleanup()
            print("PASS 无配置首次启动保留 /setup，SIGTERM 停止全部服务")
        finally:
            stop(process)

        # 拒绝占用端口，既有服务仍可使用；不尝试杀掉其它开发终端。
        with socket.socket() as existing:
            existing.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            existing.bind(("127.0.0.1", int(env["SLORAY_DEV_API_PORT"])))
            existing.listen()
            assert run().wait(timeout=5) != 0
            existing.getsockname()
        primary = fixture / ".dev-cache/runtime/config.json"
        primary.parent.mkdir(parents=True)
        primary.write_text("{}")
        legacy.write_text('{"secret":"must-not-be-printed"}')
        assert run().wait(timeout=5) != 0
        output.flush()
        assert "先停止旧开发终端" in (fixture / "output.log").read_text()
        assert "请显式指定 SLORAY_CONFIG_FILE" in (fixture / "output.log").read_text()
        print("PASS 端口冲突及两份配置时明确拒绝启动，不接管既有进程")
