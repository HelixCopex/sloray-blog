#!/usr/bin/env python3
"""隔离 PG/SSR 验证站点标识、版本冲突和可编排首页。"""
import copy
import json
from concurrent.futures import ThreadPoolExecutor
from smoke_support import App

with App() as app:
    identity = app.json('/api/v1/site', cookie=False)[2]['identity']
    assert identity['site_name'] == '静线数据库'
    form = {'site_name': '微光书房', 'favicon_url': '/favicon.svg', 'version': identity['version'], 'csrf_token': app.csrf}
    config_before = (app.root / 'config.json').read_text()
    assert app.request('/admin/settings/site', form=form)[0] in [303, 401, 403]
    assert app.request('/admin/settings/site', cookie=app.cookie, form=form | {'csrf_token': 'bad'})[0] == 403
    assert app.request('/admin/settings/site', cookie=app.cookie, form=form | {'favicon_url': 'javascript:alert(1)'})[0] == 400
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: app.request('/admin/settings/site', cookie=app.cookie, form=form)[0], range(2)))
    assert sorted(results) == [200, 409], results
    assert (app.root / 'config.json').read_text() == config_before
    identity = app.json('/api/v1/site', cookie=False)[2]['identity']
    assert identity['site_name'] == '微光书房' and identity['version'] == 2
    assert app.sql("SELECT count(*) FROM outbox WHERE topic='cdn.purge' AND payload::text LIKE '%rss.xml%'") == '1'
    package = copy.deepcopy(app.json('/api/v1/theme', cookie=False)[2]['package'])
    package['layout']['home_rows'] = [
        {'kind': 'widgets', 'title': '阅读之前', 'widgets': [{'kind': 'note', 'title': '便笺', 'text': '<script>evil()</script>'}]},
        {'kind': 'posts', 'title': '文字档案', 'widgets': []},
    ]
    invalid = copy.deepcopy(package)
    invalid['layout']['home_rows'][0]['widgets'][0]['kind'] = 'script'
    assert app.json('/api/v1/admin/themes', 'POST', {'package': invalid})[0] == 422
    theme = app.json('/api/v1/admin/themes', 'POST', {'package': package})[2]
    assert app.json('/api/v1/admin/themes/active', 'PUT', {'theme_id': theme['id'], 'theme_version': 1, 'version': 1})[0] == 200
    app.start_web()
    html = app.request('/', web=True)[2].decode()
    assert '<title>微光书房</title>' in html and 'href="/favicon.svg"' in html
    assert '文字档案' in html and '/_server-islands/HomeWidget' in html
    assert '>首页<' not in html
    fragment = app.request('/fragments/widgets?row=0&widget=0', web=True)[2].decode()
    assert '&lt;script&gt;evil()&lt;/script&gt;' in fragment and '<script>evil()' not in fragment
    assert app.request('/fragments/widgets?row=99&widget=0', web=True)[0] == 404
    assert '微光书房' in app.request('/rss.xml', web=True)[2].decode()
    print('PASS 站点名称/Favicon 持久化、鉴权/CSRF、并发冲突、outbox、首页编排与 Astro 组件重载/文本转义')
