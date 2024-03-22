import pytest
from pathlib import Path
import subprocess


@pytest.fixture
def corruption(directory, cfv_sigfile):
    files = sorted(f for f in directory.rglob("*") if not f.is_dir())
    corrupted = files[1]
    offset = 100_000
    with corrupted.open("rb+") as f:
        f.seek(offset)
        f.write(b"a")
    return corrupted


def test_cfv_verify(directory, corruption, run, report):
    with pytest.raises(subprocess.CalledProcessError) as e:
        run(
            f"verify {directory} --report json --report-filename {report.filename}",
            stderr=subprocess.PIPE,
        )
    assert b"Failed entries found" in e.value.stderr

    data = report.load()

    assert len(data["failed"]) == 1
    [failed_entry] = data["failed"]
    assert Path(failed_entry["path"]).absolute() == corruption.absolute()
    assert failed_entry["status"] == "fail"
