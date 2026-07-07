# pylint: disable=redefined-outer-name
import subprocess
from pathlib import Path

import pytest


def test_existence_only_ignores_content_changes(directory, run, algorithm, report):
    run(f"sign -a {algorithm} .", cwd=directory)

    # Change a file's contents in place (same path, different bytes).
    (directory / "c").write_bytes(b"different-content-entirely")

    # A normal test would fail on the checksum mismatch...
    with pytest.raises(subprocess.CalledProcessError):
        run(f"test -a {algorithm} .", cwd=directory, stderr=subprocess.PIPE)

    # ...but --existence-only only cares that the file still exists.
    run(
        f"test -a {algorithm} . --existence-only "
        f"--report json --report-filename {report.filename}",
        cwd=directory,
    )
    assert report.load()["failed"] == []


def test_existence_only_detects_missing(directory, run, algorithm, report):
    run(f"sign -a {algorithm} .", cwd=directory)

    (directory / "c").unlink()

    with pytest.raises(subprocess.CalledProcessError):
        run(
            f"test -a {algorithm} . --existence-only "
            f"--report json --report-filename {report.filename}",
            cwd=directory,
            stderr=subprocess.PIPE,
        )

    data = report.load()
    assert {e["status"] for e in data["failed"]} == {"missing"}
    assert any(Path(e["path"]).name == "c" for e in data["failed"])


def test_existence_only_detects_new_file(directory, run, algorithm, report):
    run(f"sign -a {algorithm} .", cwd=directory)

    (directory / "new").write_bytes(b"stray")

    with pytest.raises(subprocess.CalledProcessError):
        run(
            f"test -a {algorithm} . --existence-only "
            f"--report json --report-filename {report.filename}",
            cwd=directory,
            stderr=subprocess.PIPE,
        )

    data = report.load()
    assert {e["status"] for e in data["failed"]} == {"unknown"}
    assert any(Path(e["path"]).name == "new" for e in data["failed"])


def test_existence_only_file_replaced_by_directory_is_missing(
    directory, run, algorithm, report
):
    run(f"sign -a {algorithm} .", cwd=directory)

    # Replace the file 'c' with a directory of the same name. The path still
    # "exists", but it is no longer the regular file that was cataloged.
    (directory / "c").unlink()
    (directory / "c").mkdir()

    with pytest.raises(subprocess.CalledProcessError):
        run(
            f"test -a {algorithm} . --existence-only "
            f"--report json --report-filename {report.filename}",
            cwd=directory,
            stderr=subprocess.PIPE,
        )

    data = report.load()
    assert {e["status"] for e in data["failed"]} == {"missing"}
    assert any(Path(e["path"]).name == "c" for e in data["failed"])
