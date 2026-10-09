#!/usr/bin/env python3
"""真实 PG 验证页面权限、版本、导航、SSR 和迁移兼容性。"""
import json
from smoke_support import App

with App() as app:
    page = {'slug':'contact','title':'联系站长','summary':'联系方式','body_md':'# 联系方式\n\n**邮件**\n\n<script>alert(1)</script>\n\n[危险](javascript:alert(1))', 'status':'draft','show_in_nav':True,'nav_order':-10}
    assert app.json('/api/v1/admin/pages','POST',page,cookie=False)[0] == 401
    assert app.json('/api/v1/admin/pages','POST',page,csrf=False)[0] == 403
    for slug in ['admin','login','search','posts','_astro','api','a/b']:
        assert app.json('/api/v1/admin/pages','POST',page|{'slug':slug})[0] == 422
    status,_,created = app.json('/api/v1/admin/pages','POST',page)
    assert status == 201 and created['version'] == 1
    identifier = created['id']
    assert app.json('/api/v1/pages/contact',cookie=False)[0] == 404
    assert all(item['slug'] != 'contact' for item in app.json('/api/v1/pages',cookie=False)[2]['items'])
    assert app.json('/api/v1/admin/pages','POST',page)[0] == 409
    status,_,author = app.json('/api/v1/admin/users','POST',{'email':'page-author@example.com','display_name':'作者','password':'author-test-password','role':'author'})
    assert status == 201
    _,headers,_ = app.request('/login')
    csrf_cookie = headers['set-cookie'].split(';',1)[0]
    _,headers,_ = app.request('/login',cookie=csrf_cookie,form={'email':author['email'],'password':'author-test-password','csrf_token':csrf_cookie.split('=',1)[1]})
    author_cookie = headers['set-cookie'].split(';',1)[0] + '; ' + csrf_cookie
    for path in ['/api/v1/admin/pages',f'/api/v1/admin/pages/{identifier}']:
        assert app.request(path,cookie=author_cookie)[0] == 403
    assert app.request(f'/api/v1/admin/pages/{identifier}',method='PUT',body=page|{'version':1},cookie=author_cookie,csrf=csrf_cookie.split('=',1)[1])[0] == 403
    app.start_web()
    assert app.request('/contact',web=True)[0] == 404
    assert app.request('/admin/pages',web=True)[0] == 302
    assert app.request('/admin/pages/new',web=True,cookie=author_cookie)[0] == 403
    assert b'/admin/posts' not in app.request('/',web=True)[2]
    assert '管理后台' in app.request('/',web=True,cookie=app.cookie)[2].decode()
    page |= {'status':'published','version':1}
    assert app.json(f'/api/v1/admin/pages/{identifier}','PUT',page)[0] == 200
    assert app.json(f'/api/v1/admin/pages/{identifier}','PUT',page)[0] == 409
    listing = app.json('/api/v1/pages?limit=1',cookie=False)[2]
    assert listing['items'][0]['slug'] == 'contact' and listing['next_cursor']
    assert app.json('/api/v1/pages?cursor='+listing['next_cursor'],cookie=False)[2]['items'][0]['slug'] == 'about'
    assert app.json('/api/v1/admin/pages?cursor='+listing['next_cursor'])[0] == 422
    status,headers,body = app.request('/contact',web=True)
    html = body.decode()
    assert status == 200 and '<h2 id="联系方式">联系方式</h2>' in html and '<strong>邮件</strong>' in html
    assert '<script>alert(1)</script>' not in html and 'href="javascript:' not in html
    assert headers['cache-control'] == 'no-store'
    assert '<loc>http://localhost:4321/contact</loc>' in app.request('/sitemap.xml',web=True)[2].decode()
    assert 'href="/contact"' in app.request('/',web=True)[2].decode()
    preview = {'body_md':page['body_md']}
    assert app.request('/admin/preview',method='POST',body=preview,web=True)[0] == 401
    assert app.request('/admin/preview',method='POST',body=preview,web=True,cookie=app.cookie)[0] == 403
    status,_,preview_body = app.request('/admin/preview',method='POST',body=preview,web=True,cookie=app.cookie,csrf=app.csrf)
    assert status == 200 and json.loads(preview_body)['html'] in html
    assert app.request('/admin/preview',method='POST',body={'body_md':'x'*512001},web=True,cookie=app.cookie,csrf=app.csrf)[0] == 422
    page |= {'slug':'contact-new','show_in_nav':False,'version':2}
    assert app.json(f'/api/v1/admin/pages/{identifier}','PUT',page)[0] == 200
    assert app.request('/contact',web=True)[0] == 404
    assert app.request('/contact-new',web=True)[0] == 200
    assert 'href="/contact-new"' not in app.request('/',web=True)[2].decode()
    page |= {'status':'draft','version':3}
    assert app.json(f'/api/v1/admin/pages/{identifier}','PUT',page)[0] == 200
    assert app.request('/contact-new',web=True)[0] == 404
    assert 'contact-new' not in app.request('/sitemap.xml',web=True)[2].decode()
    assert app.json(f'/api/v1/admin/pages/{identifier}','DELETE',{'version':3})[0] == 409
    assert app.json(f'/api/v1/admin/pages/{identifier}','DELETE',{'version':4})[0] == 204
    assert app.json(f'/api/v1/admin/pages/{identifier}')[0] == 404
    assert app.sql("SELECT count(*)>0 FROM outbox WHERE topic='cdn.purge' AND payload->'paths' ? '/contact'") == 't'
    print('PASS 页面管理员权限/CSRF、保留地址、版本冲突、游标、发布/改址/撤回/删除、导航和 sitemap')
    print('PASS 页面 SSR 与 Markdown 预览一致、脚本/危险链接清洗、预览认证与长度限制、purge outbox')
