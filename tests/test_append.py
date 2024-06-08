import subprocess
import pytest


def test_unknown_files_warning(directory, run, algorithm, random_data_gen, report):
    run(f"sign -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())
    with pytest.raises(subprocess.CalledProcessError) as e:
        run(
            f"test {directory} --report json --report-filename {report.filename}",
            stderr=subprocess.PIPE,
        )
    assert b"Unknown entries found" in e.value.stderr
    rep = report.load()
    [entry] = rep["failed"]
    assert entry["path"] == f"{directory / 'new_file'}"
    assert entry["status"] == "unknown"


def test_append(directory, run, algorithm, random_data_gen):
    run(f"sign -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())
    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" not in f.read()
    run(f"append {directory}")
    with catalog.path.open() as f:
        assert "new_file" in f.read()
