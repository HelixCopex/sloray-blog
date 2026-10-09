#!/usr/bin/env python3
"""用真实 PG 验证开发数据库的首次初始化、重复启动和重启保留数据。"""
import os
import pathlib
import subprocess
import tempfile
import urllib.parse
from smoke_support import ROOT, port

with tempfile.TemporaryDirectory(prefix="sloray-dev-db-") as directory:
    root = pathlib.Path(directory) / "private database"
    env = os.environ | {"SLORAY_DEV_DB_DIR": str(root), "SLORAY_DEV_DB_PORT": str(port())}

    def control(action, check=True):
        return subprocess.run([str(ROOT / "scripts/dev-db.sh"), action], env=env,
                              check=check, capture_output=True, text=True, timeout=45)

    try:
        assert control("status", check=False).returncode != 0
        control("start")
        first = control("url").stdout.strip()
        parts = urllib.parse.urlsplit(first)
        assert parts.hostname == "127.0.0.1" and parts.path == "/sloray"
        assert parts.port == int(env["SLORAY_DEV_DB_PORT"])
        assert (root.stat().st_mode & 0o777) == 0o700
        assert ((root / "data").stat().st_mode & 0o777) == 0o700
        assert ((root / "password").stat().st_mode & 0o777) == 0o600
        control("start")
        assert first == control("url").stdout.strip(), "重复启动不能更换密码"
        sql_env = env | {"PGHOST": "127.0.0.1", "PGPORT": str(parts.port),
                         "PGUSER": parts.username, "PGPASSWORD": parts.password, "PGDATABASE": "sloray"}

        def sql(statement):
            return subprocess.run(["psql", "-v", "ON_ERROR_STOP=1", "-tAc", statement],
                                  env=sql_env, check=True, capture_output=True, text=True).stdout.strip()

        sql("CREATE TABLE dev_marker(value text); INSERT INTO dev_marker VALUES('keep')")
        control("stop")
        assert control("url", check=False).returncode != 0
        del env["SLORAY_DEV_DB_PORT"]
        control("start")
        assert first == control("url").stdout.strip(), "重启需保留自定义端口和密码"
        assert sql("SELECT value FROM dev_marker") == "keep"
        control("status")
        print("PASS 开发数据库首次初始化、TCP 密码连接、私有权限、重复启动与重启保留数据")
    finally:
        control("stop", check=False)
