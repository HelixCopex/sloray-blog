"""仅用于本地集成验证的 S3 / CDN 协议模拟器，不替代线上验收。"""
import base64
import hashlib
import http.server
import json
import threading
import time
import urllib.parse
from xml.sax.saxutils import escape


class Objects:
    def __enter__(self):
        self.objects = {}
        self.purges = []
        self.fail_purge = False
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'
            def log_message(self, *_args): pass
            def reply(self, status, body=b'', mime='application/xml', extra=None, length=None):
                self.send_response(status)
                self.send_header('Content-Length', str(len(body) if length is None else length))
                self.send_header('Content-Type', mime)
                self.send_header('Access-Control-Allow-Origin', '*')
                self.send_header('Access-Control-Allow-Headers', '*')
                self.send_header('Access-Control-Allow-Methods', 'PUT,GET,HEAD,OPTIONS')
                for key, value in (extra or {}).items(): self.send_header(key, value)
                self.end_headers()
                if self.command != 'HEAD': self.wfile.write(body)
            def key(self):
                return urllib.parse.unquote(urllib.parse.urlsplit(self.path).path).removeprefix('/test/').replace('/test-public/', 'public/', 1)
            def do_OPTIONS(self): self.reply(204)
            def do_PUT(self):
                key = self.key()
                body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
                query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
                checksum = self.headers.get('x-amz-checksum-sha256') or query.get('x-amz-checksum-sha256', [None])[0]
                if checksum and checksum != base64.b64encode(hashlib.sha256(body).digest()).decode():
                    return self.reply(400, b'<Error><Code>BadDigest</Code></Error>')
                if key in owner.objects and self.headers.get('If-None-Match') == '*':
                    return self.reply(412, b'<Error><Code>PreconditionFailed</Code></Error>')
                owner.objects[key] = (body, self.headers.get('Content-Type', ''), time.time(), self.headers.get('Cache-Control', ''))
                self.reply(200, extra={'ETag': '"test-etag"'})
            def do_HEAD(self):
                obj = owner.objects.get(self.key())
                if not obj: return self.reply(404)
                self.reply(200, mime=obj[1], length=len(obj[0]), extra={'ETag': '"test-etag"'})
            def do_GET(self):
                if 'list-type=2' in self.path:
                    xml = '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><IsTruncated>false</IsTruncated>'
                    for key, (body, _mime, modified, _cache) in owner.objects.items():
                        if key.startswith('private/'):
                            stamp = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(modified))
                            xml += f'<Contents><Key>{escape(key)}</Key><LastModified>{stamp}</LastModified><Size>{len(body)}</Size></Contents>'
                    return self.reply(200, (xml + '</ListBucketResult>').encode())
                if self.key().startswith('private/') and 'Authorization' not in self.headers:
                    return self.reply(403)
                obj = owner.objects.get(self.key())
                if not obj: return self.reply(404, b'<Error><Code>NoSuchKey</Code></Error>')
                body = obj[0]
                if self.headers.get('Range'):
                    body = body[:int(self.headers['Range'].split('-')[-1])+1]
                    return self.reply(206, body, obj[1], {'Content-Range': f'bytes 0-{len(body)-1}/{len(obj[0])}'})
                self.reply(200, body, obj[1])
            def do_DELETE(self):
                owner.objects.pop(self.key(), None)
                self.reply(204)
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                owner.purges.append(body)
                self.reply(500 if owner.fail_purge else 200, json.dumps({'success': not owner.fail_purge}).encode(), 'application/json')
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f'http://127.0.0.1:{self.server.server_port}'
        return self
    def __exit__(self, *_args):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
