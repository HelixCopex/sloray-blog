#!/usr/bin/env python3
"""验证媒体两阶段流程与 outbox；网络全部连接本地模拟器。"""
import hashlib
import importlib.util
import json
import struct
import subprocess
import time
import urllib.error
import urllib.request
import zlib
from smoke_support import App, ROOT

spec=importlib.util.spec_from_file_location('fake_objects',ROOT/'scripts/fake-objects.py')
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def png():
    def chunk(kind, body): return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',400,200,8,2,0,0,0))+chunk(b'tEXt',b'GPS\0PRIVATE-METADATA')+chunk(b'IDAT',zlib.compress((b'\0'+b'\x33\x66\x99'*400)*200))+chunk(b'IEND',b'')

with module.Objects() as objects, App() as app:
    config=json.loads((app.root/'config.json').read_text())
    config |= {'s3':{'endpoint':objects.url,'bucket':'test','region':'auto','access_key_id':'test-key','secret_access_key':'test-secret'},'media_public_bucket':'test-public','media_public_base_url':objects.url+'/test-public','cloudflare_zone_id':'testzone','cloudflare_api_token':'test-token','cloudflare_api_base_url':objects.url}
    (app.root/'config.json').write_text(json.dumps(config))
    time.sleep(2.2)
    body=png()
    input={'content_type':'image/png','size_bytes':len(body),'sha256':hashlib.sha256(body).hexdigest()}
    assert app.json('/api/v1/admin/media/presign','POST',input,cookie=False)[0]==401
    assert app.json('/api/v1/admin/media/presign','POST',input|{'content_type':'image/svg+xml'})[0]==422
    status,_,signed=app.json('/api/v1/admin/media/presign','POST',input)
    assert status==200,(status,signed)
    assert 'X-Amz-Algorithm=AWS4-HMAC-SHA256' in signed['upload_url']
    assert signed['headers']['if-none-match']=='*'
    assert '/test/private/' in signed['upload_url']
    request=urllib.request.Request(signed['upload_url'],data=body,headers=signed['headers'],method='PUT')
    assert urllib.request.urlopen(request).status==200
    try: urllib.request.urlopen(request); raise AssertionError('immutable key overwritten')
    except urllib.error.HTTPError as error: assert error.code==412
    status,_,media=app.json('/api/v1/admin/media','POST',{'upload_id':signed['upload_id']})
    assert status==201,(status,media)
    assert not media['ready'] and not media['variants']
    assert app.json(f"/api/v1/media/{media['id']}",cookie=False)[0]==404
    assert app.json('/api/v1/admin/media','POST',{'upload_id':signed['upload_id']})[2]['id']==media['id']
    subprocess.check_call([str(ROOT/'target/debug/sloray'),'worker','--once'],env=app.env)
    result=app.json(f"/api/v1/media/{media['id']}",cookie=False)[2]
    for key in [k for k in objects.objects if k.startswith('private/')]:
        for url in (objects.url+'/test/'+key,objects.url+'/test-public/'+key):
            try: urllib.request.urlopen(url); raise AssertionError('original publicly readable')
            except urllib.error.HTTPError as error: assert error.code in (403,404)
    assert result['ready'] and result['width']==400 and len(result['variants'])==1,result
    variants=[obj for key,obj in objects.objects.items() if key.startswith('public/media/')]
    assert variants and all(obj[0][:4]==b'RIFF' and obj[0][8:12]==b'WEBP' and b'PRIVATE-METADATA' not in obj[0] for obj in variants)
    assert app.json('/api/v1/admin/media/presign','POST',input)[2]['media']['id']==media['id']
    print('PASS 预签名直传、条件创建、防重放覆盖、两阶段登记、私有原图与验证后的 SHA-256 秒传')
    print('PASS worker 生成 WebP 派生图，元数据标记消失，登记重试幂等')
    bad=b'<svg><script>bad</script></svg>'
    status,_,signed=app.json('/api/v1/admin/media/presign','POST',input|{'size_bytes':len(bad),'sha256':hashlib.sha256(bad).hexdigest()})
    urllib.request.urlopen(urllib.request.Request(signed['upload_url'],data=bad,headers=signed['headers'],method='PUT'))
    assert app.json('/api/v1/admin/media','POST',{'upload_id':signed['upload_id']})[0]==422
    objects.fail_purge=True
    assert app.json(f"/api/v1/admin/media/{media['id']}",'DELETE',{})[0]==204
    subprocess.check_call([str(ROOT/'target/debug/sloray'),'worker','--once'],env=app.env)
    assert app.sql("SELECT attempts FROM outbox WHERE topic='media.delete'")=='1'
    assert app.json(f"/api/v1/media/{media['id']}",cookie=False)[0]==404
    objects.fail_purge=False
    app.sql("UPDATE outbox SET available_at=now()")
    subprocess.check_call([str(ROOT/'target/debug/sloray'),'worker','--once'],env=app.env)
    assert app.sql("SELECT processed_at IS NOT NULL FROM outbox WHERE topic='media.delete'")=='t'
    assert objects.purges
    objects.objects['private/orphan.png']=(b'orphan','image/png',time.time()-90000,'')
    output=subprocess.check_output([str(ROOT/'target/debug/sloray'),'media-gc'],env=app.env,text=True)
    assert 'candidate private/orphan.png' in output and 'private/orphan.png' in objects.objects
    subprocess.check_call([str(ROOT/'target/debug/sloray'),'media-gc','--apply'],env=app.env)
    assert 'private/orphan.png' not in objects.objects
    print('PASS magic bytes 拒绝伪装 SVG，删除立即隐藏，CDN 失败退避/重试，孤儿 GC 默认 dry-run')
