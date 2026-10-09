#!/usr/bin/env python3
"""覆盖 PITR 的提升竞态、WAL 误判、超时和失败日志持久化。"""
import contextlib
import importlib.util
import io
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("smoke_pitr", pathlib.Path(__file__).with_name("smoke-pitr.py"))
pitr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pitr)


class PitrTests(unittest.TestCase):
    def test_waits_for_promotion_after_read_only_connections(self):
        states = iter(["t", "t", "f"])
        observed = []

        def promoted():
            observed.append(next(states))
            return observed[-1] == "f"

        with patch.object(pitr.time, "sleep"):
            pitr.wait_for(promoted, "提升")
        self.assertEqual(observed, ["t", "t", "f"])

    def test_wait_is_bounded(self):
        with patch.object(pitr.time, "monotonic", side_effect=[0, 0.5, 1]), \
                patch.object(pitr.time, "sleep") as sleep:
            with self.assertRaisesRegex(TimeoutError, "WAL 归档.*1 秒"):
                pitr.wait_for(lambda: False, "WAL 归档", timeout=1)
        sleep.assert_called_once()

    def test_requires_exact_complete_archived_segment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            primary, wal = root / "primary", root / "wal"
            status = primary / "pg_wal/archive_status"
            status.mkdir(parents=True)
            wal.mkdir()
            segment = "000000010000000000000003"
            (wal / "000000010000000000000002").write_bytes(b"wal!")
            (wal / f"{segment}.backup").write_text("backup")
            self.assertFalse(pitr.archived(primary, wal, segment, 4))
            (wal / segment).write_bytes(b"wal!")
            self.assertFalse(pitr.archived(primary, wal, segment, 4))
            (status / f"{segment}.done").touch()
            (wal / segment).write_bytes(b"wa")
            self.assertFalse(pitr.archived(primary, wal, segment, 4))
            (wal / segment).write_bytes(b"wal!")
            self.assertTrue(pitr.archived(primary, wal, segment, 4))

    def test_failure_keeps_logs_after_temporary_cluster_cleanup(self):
        with tempfile.TemporaryDirectory() as destination, tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "wal").mkdir()
            (root / "primary.log").write_text("archiver failed\n")
            (root / "restored.log").write_text("recovery waiting\n")
            (root / "secret-data").write_text("do not copy database files")
            with patch.dict(os.environ, {"SLORAY_PITR_LOG_DIR": destination}), \
                    contextlib.redirect_stderr(io.StringIO()) as output:
                pitr.save_diagnostics(root, {"required_wal": "000000010000000000000003"}, ["timeout"])
            shutil.rmtree(root)
            saved = pathlib.Path(destination)
            self.assertEqual((saved / "restored.log").read_text(), "recovery waiting\n")
            self.assertIn("required_wal", (saved / "diagnostics.json").read_text())
            self.assertFalse((saved / "secret-data").exists())
            self.assertIn("archiver failed", output.getvalue())

    def test_failed_commands_report_captured_stderr(self):
        error = subprocess.CalledProcessError(1, ["psql"], output="", stderr="FATAL: recovery failed")
        self.assertIn("FATAL: recovery failed", pitr.describe_error(error))


if __name__ == "__main__":
    unittest.main(verbosity=2)
