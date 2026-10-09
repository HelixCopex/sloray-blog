#!/usr/bin/env python3
"""检索：中文分词、语法、keyset、手工修复失效和 1000 篇文章压测。"""
import concurrent.futures
import statistics
import subprocess
import time
import urllib.parse
from smoke_support import App, ROOT

with App() as app:
    for i in range(7):
        article={"slug":f"search-{i}","title":"分布式博客架构","body_md":"Rust 数据库事务设计","status":"published" if i<6 else "draft"}
        assert app.json("/api/v1/admin/posts","POST",article)[0]==201
    query=lambda q,cursor=None: app.json("/api/v1/search?"+urllib.parse.urlencode({"q":q,"limit":2}|({"cursor":cursor} if cursor else {})),cookie=False)
    cursor=None
    seen=[]
    while True:
        status,_,page=query("博客",cursor)
        assert status==200,(status,page)
        seen.extend(item["id"] for item in page["items"])
        cursor=page["next_cursor"]
        if not cursor:break
    assert len(seen)==len(set(seen))==6
    cursor=query("博客")[2]["next_cursor"]
    assert query("事务",cursor)[0]==422
    assert query("博客 不存在的关键词")[2]["relaxed"] is True
    assert query("博客 -rust")[2]["items"]==[]
    assert query("& | !'")[0]==200
    print("PASS 中文 FTS，草稿排除、同排序值 keyset 分页、游标范围校验、OR 兜底和排除语法")
    app.sql("UPDATE posts SET title='针叶树森林',body_md='杉树' WHERE slug='search-0'")
    assert app.sql("SELECT search_vector IS NULL FROM posts WHERE slug='search-0'")=="t"
    subprocess.check_call([str(ROOT/"target/debug/sloray"),"worker","--once"],env=app.env)
    assert query("森林")[2]["items"][0]["slug"]=="search-0"
    assert query("森林 -rust")[2]["items"][0]["slug"]=="search-0"
    print("PASS SQL 绕过应用修改使旧向量失效；worker 从原文修复后中文子词可查")
    app.sql("INSERT INTO posts(id,slug,title,summary,body_md,status,published_at,author_id) SELECT gen_random_uuid(),'load-'||n,'博客事务设计 '||n,'Postgres 检索','分布式数据库与博客架构设计','published',now(),(SELECT id FROM users LIMIT 1) FROM generate_series(1,1000) n")
    subprocess.check_call([str(ROOT/"target/debug/sloray"),"rebuild-search"],env=app.env)
    app.sql("ANALYZE posts")
    assert app.sql("SELECT count(*) FROM posts WHERE search_vector IS NULL")=="0"
    assert app.sql("SELECT count(*) FROM pg_indexes WHERE indexname='posts_search_idx' AND indexdef LIKE '%USING gin%'")=="1"
    def measure(_):
        start=time.perf_counter()
        assert query("博客 架构")[0]==200
        return (time.perf_counter()-start)*1000
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        timings=list(pool.map(measure,range(100)))
    print(f"PASS 1007 篇文章 / 8 并发 / 100 请求：搜索 P95={statistics.quantiles(timings,n=100)[94]:.2f}ms，中位数={statistics.median(timings):.2f}ms（本机临时 PG）")
