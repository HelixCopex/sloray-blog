#!/usr/bin/env python3
"""真实 PG / SSR 验证主题包、发布快照、隔离与并发版本。"""
import copy
import re
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor
from smoke_support import App, ROOT

with App() as app:
    status, _, state = app.json('/api/v1/theme', cookie=False)
    assert status == 200 and state['active_theme_id'] is None and state['version'] == 1
    frontend_default = json.loads(subprocess.check_output(['node', '--input-type=module', '-e', "import { defaultTheme } from './web/src/lib/theme.mjs'; process.stdout.write(JSON.stringify(defaultTheme))"], cwd=ROOT, text=True))
    assert state['package'] == frontend_default, 'Rust 与 Astro 内置主题必须保持一致'
    package = copy.deepcopy(state['package'])
    package['name'] = '测试主题'
    package['palette'] = 'teal'
    package['layout'] |= {'width': 1440, 'columns': 3, 'navigation': 'side', 'home_blocks': ['posts', 'hero', 'pages'], 'hero_title': '自定义首页'}
    package['css'] = '.hero {border: 3px solid red;} /* </style><script>alert(1)</script> */'
    payload = {'package': package}
    assert app.json('/api/v1/admin/themes', 'POST', payload, cookie=False)[0] == 401
    assert app.json('/api/v1/admin/themes', 'POST', payload, csrf=False)[0] == 403
    invalids = [package | {'schema_version': 2}, package | {'javascript': 'evil()'}, package | {'css': 'x' * 65537}, package | {'palette': 'evil'}]
    for layout in [{'width': 1601}, {'columns': 0}, {'home_blocks': ['hero']}, {'home_blocks': ['posts', 'posts']}, {'navigation': 'script'}]:
        invalids.append(package | {'layout': package['layout'] | layout})
    for invalid in invalids:
        assert app.json('/api/v1/admin/themes', 'POST', {'package': invalid})[0] in [400, 422]
    status, _, theme = app.json('/api/v1/admin/themes', 'POST', payload)
    assert status == 201 and theme['version'] == 1
    identifier = theme['id']
    # 保存不发布；旧主题快照不会被编辑隐式覆盖。
    assert app.json('/api/v1/theme', cookie=False)[2]['active_theme_id'] is None
    assert app.json('/api/v1/admin/posts', 'POST', {'slug': 'theme-etag', 'title': '主题缓存验证', 'body_md': '正文', 'status': 'published'})[0] == 201
    app.start_web()
    _, old_headers, _ = app.request('/posts/theme-etag', web=True)
    assert app.request('/posts/theme-etag', web=True, headers={'If-None-Match': old_headers['etag']})[0] == 304
    assert app.request('/admin/themes', web=True)[0] == 403
    assert '自定义首页' not in app.request('/', web=True)[2].decode()
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': identifier, 'theme_version': 1, 'version': 1})[0], range(2)))
    assert sorted(results) == [200, 409], results
    state = app.json('/api/v1/theme', cookie=False)[2]
    assert state['version'] == 2 and state['package'] == package
    assert app.request('/posts/theme-etag', web=True, headers={'If-None-Match': old_headers['etag']})[0] == 200
    html = app.request('/', web=True)[2].decode()
    assert '自定义首页' in html and 'data-navigation="side"' in html and '--content-width:1440px' in html
    assert '</style><script>alert(1)</script>' not in html and '\\3c /style>' in html
    assert html.index('aria-label="文章列表"') < html.index('class="hero"')
    assert '/_server-islands/PageDirectory' in html
    assert app.sql("SELECT count(*) FROM outbox WHERE topic='cdn.purge'") != '0'
    # 后台使用默认主题，且 server island 请求带 Cookie；游客不能借岛屿 URL 获得主题库。
    status, _, admin_html = app.request('/admin/themes', web=True, cookie=app.cookie)
    assert status == 200 and '--content-width:1440px' not in admin_html.decode()
    island = re.search(r'/_server-islands/ThemeWorkbench\?[^"< ]+', admin_html.decode()).group(0).replace('&amp;', '&')
    assert '测试主题' not in app.request(island, web=True)[2].decode()
    assert '测试主题' in app.request(island, web=True, cookie=app.cookie)[2].decode()
    assert app.json(f'/api/v1/admin/themes/{identifier}', 'DELETE', {'version': 1})[0] == 422
    updated = package | {'name': '尚未发布的修改'}
    assert app.json(f'/api/v1/admin/themes/{identifier}', 'PUT', {'package': updated, 'version': 1})[0] == 200
    assert app.json(f'/api/v1/admin/themes/{identifier}', 'PUT', {'package': updated, 'version': 1})[0] == 409
    assert app.json('/api/v1/theme', cookie=False)[2]['package']['name'] == '测试主题'
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': identifier, 'theme_version': 1, 'version': 2})[0] == 409
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': identifier, 'version': 2})[0] == 422
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': identifier, 'theme_version': 2, 'version': 2})[0] == 200
    assert app.json('/api/v1/theme', cookie=False)[2]['package']['name'] == '尚未发布的修改'
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': None, 'version': 3})[0] == 200
    assert app.json('/api/v1/admin/themes', cookie=False)[0] == 401
    assert app.json(f'/api/v1/admin/themes/{identifier}', 'DELETE', {'version': 1})[0] == 409
    assert app.json(f'/api/v1/admin/themes/{identifier}', 'DELETE', {'version': 2})[0] == 204
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': identifier, 'theme_version': 2, 'version': 4})[0] == 404
    assert not app.json('/api/v1/admin/themes')[2]
    author = app.json('/api/v1/admin/users', 'POST', {'email': 'theme-author@example.com', 'display_name': '作者', 'password': 'author-test-password', 'role': 'author'})[2]
    _, headers, _ = app.request('/login')
    csrf_cookie = headers['set-cookie'].split(';', 1)[0]
    _, headers, _ = app.request('/login', cookie=csrf_cookie, form={'email': author['email'], 'password': 'author-test-password', 'csrf_token': csrf_cookie.split('=', 1)[1]})
    author_cookie = headers['set-cookie'].split(';', 1)[0] + '; ' + csrf_cookie
    for path, method, body in [('/api/v1/admin/themes', 'GET', None), ('/api/v1/admin/themes', 'POST', payload), ('/api/v1/admin/themes/active', 'PUT', {'theme_id': None, 'version': 4})]:
        assert app.request(path, method=method, body=body, cookie=author_cookie, csrf=csrf_cookie.split('=', 1)[1])[0] == 403
    assert app.request('/admin/themes', web=True, cookie=author_cookie)[0] == 403
    print('PASS 主题包校验、鉴权/CSRF、并发启用冲突、发布快照、SSR 布局、样式注入隔离、群岛鉴权、恢复与删除')
