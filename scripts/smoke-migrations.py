#!/usr/bin/env python3
"""隔离数据库验证所有新增迁移的逆序、重放与空库 fresh。"""
import subprocess
from smoke_support import App, ROOT
with App() as app:
    # 停 API，避免在刻意移除 schema 时后台连接重载制造噪声。
    app.processes[0].terminate(); app.processes[0].wait(timeout=15)
    def cli(*args):
        return subprocess.check_output([str(ROOT/'target/debug/sloray'),'migrate',*args],env=app.env,text=True)
    status=cli('status')
    assert status.count('Applied')==7,status
    cli('down','7')
    assert app.sql("SELECT count(*) FROM pg_tables WHERE schemaname='public'")=='1'
    cli('up')
    assert app.sql("SELECT count(*) FROM pg_tables WHERE schemaname='public'")=='21'
    cli('down','1'); cli('up')
    cli('fresh')
    assert cli('status').count('Applied')==7
    print('PASS 七条迁移 Applied；down all → up、down 1 → up、fresh，21 张表重建成功')
