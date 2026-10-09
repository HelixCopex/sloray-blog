#!/usr/bin/env python3
"""一次隔离的物理备份/WAL/按时间恢复演练，不访问既有 PG 数据目录。"""
import pathlib
import shutil
import subprocess
import tempfile
import time
from smoke_support import port

def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs).stdout.strip()

with tempfile.TemporaryDirectory(prefix='sloray-pitr-') as directory:
    root=pathlib.Path(directory)
    primary=root/'primary'
    restored=root/'restored'
    wal=root/'wal'
    wal.mkdir()
    first,second=port(),port()
    started=[]
    try:
        run('initdb','-D',str(primary),'--auth=trust','--username=sloray','--encoding=UTF8','--no-locale')
        with (primary/'postgresql.conf').open('a') as config:
            config.write(f"\nlisten_addresses='127.0.0.1'\nport={first}\nunix_socket_directories='{root}'\narchive_mode=on\narchive_command='test ! -f {wal}/%f && cp %p {wal}/%f'\n")
        run('pg_ctl','-D',str(primary),'-l',str(root/'primary.log'),'start')
        started.append(primary)
        def sql(statement, p=first):
            return run('psql','-h','127.0.0.1','-p',str(p),'-U','sloray','-d','postgres','-v','ON_ERROR_STOP=1','-tAc',statement)
        sql("CREATE TABLE restore_marker(value text PRIMARY KEY); INSERT INTO restore_marker VALUES('base')")
        run('pg_basebackup','-h','127.0.0.1','-p',str(first),'-U','sloray','-D',str(restored),'-X','stream','--checkpoint=fast')
        if shutil.which('pg_verifybackup'):
            run('pg_verifybackup',str(restored))
        sql("INSERT INTO restore_marker VALUES('keep')")
        target=sql('SELECT clock_timestamp()')
        time.sleep(0.15)
        sql("INSERT INTO restore_marker VALUES('discard')")
        sql('SELECT pg_switch_wal()')
        for _ in range(100):
            if len(list(wal.glob('0'*8+'*')))>=2: break
            time.sleep(0.1)
        assert len(list(wal.iterdir()))>=2,'WAL 归档未完成'
        with (restored/'postgresql.conf').open('a') as config:
            config.write(f"\nport={second}\narchive_mode=off\nrestore_command='cp {wal}/%f %p'\nrecovery_target_time='{target}'\nrecovery_target_action='promote'\n")
        (restored/'recovery.signal').touch()
        run('pg_ctl','-D',str(restored),'-l',str(root/'restored.log'),'start')
        started.append(restored)
        assert sql("SELECT string_agg(value,',' ORDER BY value) FROM restore_marker",second)=='base,keep'
        assert sql('SELECT pg_is_in_recovery()',second)=='f'
        print('PASS 物理基础备份 + WAL 归档 + recovery_target_time 恢复并提升；保留目标前记录，排除目标后记录')
    finally:
        for data in reversed(started):
            run('pg_ctl','-D',str(data),'-m','fast','stop')
