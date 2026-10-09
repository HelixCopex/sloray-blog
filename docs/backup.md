# 备份与恢复

逻辑备份和 PITR 解决不同恢复目标。数据库、私有原图、运行时配置与运维凭据都要
有受限的异机副本；公开 WebP 可从原图重建。恢复前选定时间点，先在隔离环境演练，
验证后才由运维切换入口。仓库测试不接触生产数据库目录。

## 逻辑备份

`deploy/scripts/backup-postgres.sh` 从环境读取 SLORAY_DATABASE_URL；使用
pg_dump 客户端大版本不低于服务器。BACKUP_DIR 默认 /var/backups/sloray，
目录 0700、文件 0600，RETAIN_DAYS 默认 14 且必须是正整数。
密码只传入子进程环境；PGPASSWORD 不应由日志或监控采集。

运维可通过 transient systemd 服务使用受限的环境文件：

```bash
sudo systemd-run --wait --collect \
  -p EnvironmentFile=/etc/sloray/api.env \
  /srv/sloray/current/deploy/scripts/backup-postgres.sh
```

由 cron 或既有调度器定期执行相同操作。仅使用 JSON 配置的部署需从该私有文件
读取 database_url 到备份进程环境，不要复制密码到命令行、日志或公开配置。
备份脚本支持 URI 常用的 sslmode/sslrootcert/sslcert/sslkey 等参数；未知参数失败关闭。

产物先写临时文件，pg_dump 成功并且 pg_restore --list 可读后才命名为完成文件，
另生成 SHA-256 清单。配置 SLORAY_BACKUP_BUCKET 时必须有 rclone 或 aws：
`SLORAY_BACKUP_UPLOADER=rclone|aws` 可显式选择，rclone 使用已配置的 remote:路径，
aws 使用 s3://bucket/prefix 与主机自己的 endpoint/凭据配置。
转储与 SHA-256 均上传成功后才清理过保留期的本站旧文件。远端失败保留旧备份，
不能把“本地转储成功”报告成“远端备份完成”。对象存储版本保留与异机生命周期需另配置。

校验目录与 hash 不能代替恢复：在新数据库（不要用生产库名）执行：

```bash
cd /var/backups/sloray
sha256sum --check <文件>.dump.sha256
createdb --template=template0 --encoding=UTF8 sloray_restore_drill
pg_restore --exit-on-error --no-owner --no-privileges \
  --dbname=sloray_restore_drill <文件>.dump
```

连接凭据使用受限的 .pgpass/PGSERVICE 配置或子进程环境。核对 users、posts、
revisions、comments、media、outbox、会话与迁移记录，再启动隔离 API/SSR 验证读取。
演练复制的会话与 setup token 是秘密；恢复对外服务前轮换部署凭据并撤销旧会话。
对象操作 worker 先保持停止，避免演练重放生产删除或 CDN 清理任务。

`scripts/smoke-backup.py` 创建两个临时数据库，真实转储/恢复并逐表核对全部 17 张表。
`test-backup.sh` 验证上传失败不清理旧备份、密码不进入转储参数、成功清理与非法配置拒绝。

## PITR

pg_dump 不包含 WAL 恢复需要的物理状态。PITR 必须有目标之前的物理基础备份，
以及从基础备份开始到目标的完整 WAL 链；不能恢复缺失归档段。
[PostgreSQL 连续归档与 PITR 手册](https://www.postgresql.org/docs/18/continuous-archiving.html)
是宿主配置的依据，选择与服务器相同大版本的文档。

运维需要落实：

1. 启用 replica 或更高 wal_level、archive_mode，选择可持久化的异机归档目录/服务。
   归档器成功才返回零，拒绝覆盖不同内容的已有归档；不能仅复制到同一故障磁盘。
2. 验证归档可恢复后用 pg_basebackup 取得基础备份，并用 pg_verifybackup 校验。
   同时保存 PostgreSQL 配置、角色/权限与恢复凭据，避免混用不同集群的 WAL。
3. 持续关注归档失败、积压与磁盘容量；按基础备份链清理 WAL，不能直接套逻辑备份的天数清理。
4. 在隔离实例恢复基础备份，配置 restore_command、recovery_target_time 和
   recovery_target_action。创建 recovery.signal，使用不同端口且禁用生产 worker。
5. 检查目标之前记录存在、之后记录不存在，确认数据库已经完成恢复，再决定切换。

生产连续归档和保留策略由主机运维管理；仓库不自动重写现有 PostgreSQL 配置。
`scripts/smoke-pitr.py` 使用两个临时集群、物理基础备份、本地 WAL 归档和时间点恢复，
验证 base/keep 被保留、discard 被排除及恢复后提升。它证明恢复流程可执行，
不能证明生产异机归档、对象存储备份或季度调度已经配置。

## 演练记录

每季度至少一次，记录备份时间、目标时间、hash、数据库/工具版本、恢复耗时、
逐表核对与页面读写结果。RPO/RTO 以主机实测与备份频率确定，不从本地小数据集推断。
本次隔离执行输出见 [verification.md](verification.md)。
