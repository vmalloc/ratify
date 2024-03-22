import pytest
from pathlib import Path
import subprocess


@pytest.fixture
def deleted_file(directory, cfv_sigfile):
    files = sorted(f for f in directory.rglob("*") if not f.is_dir())
    missing = files[1]
    missing.unlink()
    return missing


def test_missing_file(directory, deleted_file, run, report):
    with pytest.raises(subprocess.CalledProcessError) as e:
        run(
            f"verify {directory} --report json --report-filename {report.filename}",
            stderr=subprocess.PIPE,
        )
    assert b"Missing entries found" in e.value.stderr
    data = report.load()

    assert len(data["failed"]) == 1
    [failed_entry] = data["failed"]
    assert failed_entry["status"] == "missing"
    assert Path(failed_entry["path"]).absolute() == deleted_file.absolute()
