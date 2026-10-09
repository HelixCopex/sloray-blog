#!/usr/bin/env python3
"""真实数据库与 SSR，使用隔离同源代理运行 Chromium 交互和 axe 审计。"""
import http.client
import http.server
import importlib.util
import json
import os
import pathlib
import subprocess
import threading
import time
from smoke_support import App, ROOT

def load(name, file):
    spec = importlib.util.spec_from_file_location(name, file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

objects_module = load('objects', ROOT/'scripts/fake-objects.py')
# 创建 PNG，不导入会执行媒体测试的 smoke-media.py。
import struct
import zlib
def png():
    def chunk(k,b): return struct.pack('>I',len(b))+k+b+struct.pack('>I',zlib.crc32(k+b))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',400,200,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+b'\x33\x66\x99'*400)*200))+chunk(b'IEND',b'')

with objects_module.Objects() as objects, App() as app:
    config=json.loads((app.root/'config.json').read_text())
    config |= {'s3':{'endpoint':objects.url,'bucket':'test','region':'auto','access_key_id':'test-key','secret_access_key':'test-secret'},'media_public_bucket':'test-public','media_public_base_url':objects.url+'/test-public'}
    (app.root/'config.json').write_text(json.dumps(config))
    time.sleep(2.2)
    assert app.json('/api/v1/admin/posts','POST',{'slug':'browser-seed','title':'浏览器测试文章','body_md':'# 中文浏览器导航测试\n\n文字留在这里。\n\n```js\nconst reading = true;\n```','status':'published'})[0]==201
    paragraphs = ('在阅读中留出呼吸的空间，让文字沿着清晰的路径展开。' * 8 + '\n\n') * 12
    guide = '# 第一章\n\n' + paragraphs + '## 细节与注释\n\n' + paragraphs + '# 第二章\n\n' + paragraphs + '# 第三章\n\n' + paragraphs
    assert app.json('/api/v1/admin/posts','POST',{'slug':'toc-guide','title':'阅读随动指南','body_md':guide,'status':'published'})[0]==201
    assert app.json('/api/v1/admin/pages','POST',{'slug':'toc-page','title':'页面阅读指南','body_md':guide,'status':'published','show_in_nav':False,'nav_order':0})[0]==201
    app.start_web()
    class Proxy(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args): pass
        def forward(self):
            rust=self.path.startswith(('/api/', '/login', '/logout', '/setup', '/admin/settings', '/healthz', '/readyz'))
            conn=http.client.HTTPConnection('127.0.0.1',app.api_port if rust else app.web_port,timeout=30)
            body=self.rfile.read(int(self.headers.get('Content-Length','0')))
            headers={k:v for k,v in self.headers.items() if k.lower() not in ('host','connection','accept-encoding')}
            conn.request(self.command,self.path,body,headers)
            result=conn.getresponse()
            data=result.read()
            self.send_response(result.status)
            for k,v in result.getheaders():
                if k.lower() not in ('transfer-encoding','connection','content-length'): self.send_header(k,v)
            self.send_header('Content-Length',str(len(data)))
            self.end_headers()
            if self.command!='HEAD':
                try: self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError): pass
            conn.close()
        do_GET=do_HEAD=do_POST=do_PUT=do_DELETE=forward
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Proxy)
    thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
    stopped=threading.Event()
    def worker():
        while not stopped.wait(1):
            subprocess.run([str(ROOT/'target/debug/sloray'),'worker','--once'],env=app.env,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    worker_thread=threading.Thread(target=worker,daemon=True); worker_thread.start()
    image=app.root/'cover.png'; image.write_bytes(png())
    try:
        env=app.env|{'SLORAY_BROWSER_URL':f'http://127.0.0.1:{server.server_port}','SLORAY_BROWSER_IMAGE':str(image)}
        subprocess.run([os.environ.get('NODE_BIN','node'),str(ROOT/'web/scripts/browser-test.mjs')],env=env,check=True)
    finally:
        stopped.set(); worker_thread.join(timeout=40)
        server.shutdown(); server.server_close(); thread.join()
