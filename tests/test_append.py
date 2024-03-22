import subprocess
import pytest


def test_unknown_files_warning(directory, run, algorithm, random_data_gen, report):
    run(f"create -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())
    with pytest.raises(subprocess.CalledProcessError) as e:
        run(
            f"verify {directory} --report json --report-filename {report.filename}",
            stderr=subprocess.PIPE,
        )
    assert b"Unknown entries found" in e.value.stderr


def test_append(directory, run, algorithm, random_data_gen):
    run(f"create -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())
    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" not in f.read()
    run(f"append {directory}")
    with catalog.path.open() as f:
        assert "new_file" in f.read()
