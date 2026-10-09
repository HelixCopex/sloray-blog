#!/usr/bin/env python3
"""真实 Postgres 验证审核、限流、并发计数和重复执行 worker。"""
import concurrent.futures
import json
import subprocess
from smoke_support import App, ROOT

with App() as app:
    article = {"slug":"activity","title":"活动测试","body_md":"正文","status":"published"}
    status, _, post = app.json("/api/v1/admin/posts","POST",article)
    assert status==201, (status,post)
    pid=post["id"]
    comment={"guest_name":"读者","body_md":"<script>不能执行</script>","parent_id":None}
    assert app.json("/api/v1/posts/activity/comments","POST",comment,csrf=False)[0]==403
    assert app.json("/api/v1/posts/activity/comments","POST",comment|{"website":"spam"})[0]==202
    assert app.json("/api/v1/admin/comments")[2]["items"]==[]
    assert app.json("/api/v1/posts/activity/comments","POST",comment)[0]==202
    assert app.json("/api/v1/posts/activity/comments",cookie=False)[2]["items"]==[]
    queued=app.json("/api/v1/admin/comments")[2]["items"]
    cid=queued[0]["id"]
    assert app.json(f"/api/v1/admin/comments/{cid}","PUT",{"status":"approved"})[0]==204
    assert len(app.json("/api/v1/posts/activity/comments",cookie=False)[2]["items"])==1
    assert app.json("/api/v1/posts/activity/comments","POST",comment|{"parent_id":cid})[0]==202
    assert app.json("/api/v1/posts/activity/comments","POST",comment)[0]==202
    assert app.json("/api/v1/posts/activity/comments","POST",comment)[0]==429
    print("PASS 评论先审后发、蜜罐丢弃、回复校验、CSRF 和共享数据库限流")
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        results=list(pool.map(lambda _:app.json(f"/api/v1/posts/{pid}/view","POST",{})[0],range(24)))
    assert results==[204]*24,results
    assert app.json(f"/api/v1/posts/{pid}/stats")[2]["view_count"]==1
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        results=list(pool.map(lambda _:app.json(f"/api/v1/posts/{pid}/like","PUT",{})[0],range(12)))
    assert results==[204]*12,results
    assert app.json(f"/api/v1/posts/{pid}/stats")[2]=={"view_count":1,"like_count":1,"liked":True}
    for _ in range(2):
        assert app.json(f"/api/v1/posts/{pid}/like","DELETE",{})[0]==204
    assert app.json(f"/api/v1/posts/{pid}/stats")[2]["like_count"]==0
    app.sql("UPDATE post_stats SET view_count=999,like_count=999")
    output=subprocess.check_output([str(ROOT/"target/debug/sloray"),"rebuild-stats"],env=app.env,text=True)
    assert "1" in output and app.json(f"/api/v1/posts/{pid}/stats")[2]["view_count"]==1
    print("PASS 24 次并发浏览仅计一次，重复点赞/取消幂等，计数可从权威记录重建")
    app.sql("UPDATE posts SET status='scheduled',published_at=now()-interval '1 second'")
    assert app.json("/api/v1/posts/activity",cookie=False)[0]==404
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        list(pool.map(lambda _:subprocess.check_call([str(ROOT/"target/debug/sloray"),"worker","--once"],env=app.env),range(2)))
    assert app.json("/api/v1/posts/activity",cookie=False)[2]["version"]==2
    assert app.request("/login",form={"email":"admin@example.com","password":"a-test-only-password"})[0]==403
    print("PASS 两个 worker 只发布一次；HTML 登录缺少 CSRF 被拒绝")
