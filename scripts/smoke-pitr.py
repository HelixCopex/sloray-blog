#!/usr/bin/env python3
"""一次隔离的物理备份/WAL/按时间恢复演练，不访问既有 PG 数据目录。"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
from smoke_support import port


def run(*args, timeout=60, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True,
                          timeout=timeout, **kwargs).stdout.strip()


def wait_for(check, description, timeout=60):
    deadline = time.monotonic() + timeout
    while True:
        if check():
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"{description}：等待 {timeout} 秒后仍未完成")
        time.sleep(min(0.1, remaining))


def archived(primary, wal, segment, size):
    """同时确认归档器成功标记和指定完整 WAL，排除 .backup 或其它段。"""
    archive = wal / segment
    return ((primary / "pg_wal/archive_status" / f"{segment}.done").is_file()
            and archive.is_file() and archive.stat().st_size == size)


def save_diagnostics(root, state, errors):
    destination = pathlib.Path(os.environ["SLORAY_PITR_LOG_DIR"]) if os.environ.get(
        "SLORAY_PITR_LOG_DIR") else pathlib.Path(tempfile.mkdtemp(prefix="sloray-pitr-logs-"))
    destination.mkdir(parents=True, exist_ok=True)
    state = state | {"archived_files": sorted(p.name for p in (root / "wal").iterdir()),
                     "errors": errors}
    (destination / "diagnostics.json").write_text(
        json.dumps(state, ensure_ascii=False, indent=2) + "\n")
    print(f"PITR 失败诊断已保存到 {destination}", file=sys.stderr)
    print(json.dumps(state, ensure_ascii=False, indent=2), file=sys.stderr)
    for name in ("primary.log", "restored.log"):
        source = root / name
        if source.is_file():
            shutil.copy2(source, destination / name)
            print(f"--- {name}（最后 80 行）---", file=sys.stderr)
            print("\n".join(source.read_text(errors="replace").splitlines()[-80:]), file=sys.stderr)
    return destination


def describe_error(error):
    details = [f"{type(error).__name__}: {error}"]
    for name in ("stdout", "stderr"):
        output = getattr(error, name, None)
        if output:
            if isinstance(output, bytes):
                output = output.decode(errors="replace")
            details.append(f"{name}:\n{output.strip()}")
    return "\n".join(details)


def exercise(root):
    primary, restored, wal = (root / name for name in ("primary", "restored", "wal"))
    wal.mkdir()
    first, second = port(), port()
    started, errors, state = [], [], {"primary_port": first, "restored_port": second}

    def start(data, log):
        # start 超时也可能已启动服务；finally 仍须停止该隔离集群。
        started.append(data)
        run("pg_ctl", "-D", str(data), "-l", str(root / log), "-w", "-t", "60", "start", timeout=65)

    def sql(statement, p=first):
        return run("psql", "-h", "127.0.0.1", "-p", str(p), "-U", "sloray", "-d", "postgres",
                   "-v", "ON_ERROR_STOP=1", "-tAc", statement, timeout=5,
                   env=os.environ | {"PGCONNECT_TIMEOUT": "5", "PGOPTIONS": "-c statement_timeout=5000"})

    try:
        run("initdb", "-D", str(primary), "--auth=trust", "--username=sloray", "--encoding=UTF8", "--no-locale")
        with (primary / "postgresql.conf").open("a") as config:
            config.write(f"\nlisten_addresses='127.0.0.1'\nport={first}\nunix_socket_directories='{root}'\n"
                         f"archive_mode=on\narchive_command='test ! -f {wal}/%f && cp %p {wal}/%f'\n")
        start(primary, "primary.log")
        sql("CREATE TABLE restore_marker(value text PRIMARY KEY); INSERT INTO restore_marker VALUES('base')")
        run("pg_basebackup", "-h", "127.0.0.1", "-p", str(first), "-U", "sloray", "-D", str(restored),
            "-X", "stream", "--checkpoint=fast")
        if shutil.which("pg_verifybackup"):
            run("pg_verifybackup", str(restored))
        sql("INSERT INTO restore_marker VALUES('keep')")
        state["recovery_target_time"] = sql("SELECT clock_timestamp()")
        time.sleep(0.15)
        sql("INSERT INTO restore_marker VALUES('discard')")
        # pg_switch_wal() 位于段边界；pg_walfile_name() 返回刚完成的前一段。
        segment = sql("SELECT pg_walfile_name(pg_switch_wal())")
        state["required_wal"] = segment
        size = int(sql("SELECT pg_size_bytes(current_setting('wal_segment_size'))"))
        wait_for(lambda: archived(primary, wal, segment, size), f"WAL {segment} 完整归档")
        with (restored / "postgresql.conf").open("a") as config:
            config.write(f"\nport={second}\narchive_mode=off\nrestore_command='cp {wal}/%f %p'\n"
                         f"recovery_target_time='{state['recovery_target_time']}'\nrecovery_target_action='promote'\n")
        (restored / "recovery.signal").touch()
        start(restored, "restored.log")

        def promoted():
            state["last_recovery_state"] = sql("SELECT pg_is_in_recovery()", second)
            return state["last_recovery_state"] == "f"

        # hot standby 可以接受查询，必须等 PostgreSQL 自动完成恢复与提升。
        wait_for(promoted, "按目标时间恢复并自动提升")
        values = sql("SELECT string_agg(value,',' ORDER BY value) FROM restore_marker", second)
        if values != "base,keep":
            raise AssertionError(f"恢复后的记录应为 base,keep，实际为 {values!r}")
    except Exception as error:
        errors.append(describe_error(error))
        raise
    finally:
        cleanup_errors = []
        for data in reversed(started):
            if not (data / "postmaster.pid").exists():
                continue
            try:
                run("pg_ctl", "-D", str(data), "-m", "fast", "-w", "-t", "30", "stop", timeout=35)
            except Exception as error:
                cleanup_errors.append(describe_error(error))
        if errors or cleanup_errors:
            save_diagnostics(root, state, errors + cleanup_errors)
        if cleanup_errors and not errors:
            raise RuntimeError("隔离集群停止失败；详见 PITR 诊断日志")


def main():
    with tempfile.TemporaryDirectory(prefix="sloray-pitr-") as directory:
        exercise(pathlib.Path(directory))
    print("PASS 物理基础备份 + WAL 归档 + recovery_target_time 恢复并提升；保留目标前记录，排除目标后记录")


if __name__ == "__main__":
    main()
