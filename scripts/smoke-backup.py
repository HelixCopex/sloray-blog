#!/usr/bin/env python3
"""对真实应用数据库执行逻辑备份并恢复到另一个空库，核对权威记录。"""
import subprocess
import urllib.parse
import uuid
from smoke_support import App, ROOT

with App() as app:
    assert app.json('/api/v1/admin/posts','POST',{'slug':'backup','title':'恢复验证','body_md':'权威原文','status':'published'})[0]==201
    directory=app.root/'backups'
    env=app.env|{'SLORAY_DATABASE_URL':app.url,'BACKUP_DIR':str(directory),'RETAIN_DAYS':'14'}
    subprocess.check_call([str(ROOT/'deploy/scripts/backup-postgres.sh')],env=env)
    dump=next(directory.glob('*.dump'))
    assert dump.stat().st_mode & 0o777==0o600
    assert directory.stat().st_mode & 0o777==0o700
    subprocess.check_call(['sha256sum','--check',dump.name+'.sha256'],cwd=directory,stdout=subprocess.DEVNULL)
    name='sloray_restore_'+uuid.uuid4().hex
    url=urllib.parse.urlunsplit(urllib.parse.urlsplit(app.base)._replace(path='/'+name))
    app.sql(f'CREATE DATABASE "{name}" TEMPLATE template0 ENCODING \'UTF8\'',app.base)
    try:
        subprocess.check_call(['pg_restore','--exit-on-error','--no-owner','--no-privileges','--dbname',url,str(dump)],stdout=subprocess.DEVNULL)
        for table in ['users','sessions','posts','post_revisions','tags','post_tags','comments','media','post_view_daily','post_stats','likes','outbox','request_limits','privacy_secrets','search_settings','media_uploads','pages','seaql_migrations']:
            sql=f'SELECT coalesce(jsonb_agg(row_to_json(t) ORDER BY row_to_json(t)::text)::text,\'[]\') FROM "{table}" t'
            assert app.sql(sql)==app.sql(sql,url),table
        print('PASS 0600 逻辑备份、SHA-256 校验、空库完整恢复，18 张表内容与迁移记录逐表一致')
    finally:
        app.sql(f'DROP DATABASE "{name}" WITH (FORCE)',app.base)
