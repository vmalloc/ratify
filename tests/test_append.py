# pylint: disable=redefined-outer-name
import subprocess

import pytest

try:
    import pexpect
except ImportError:
    pytest.skip("pexpect is required for interactive append tests", allow_module_level=True)


def test_append_yes_flag_adds_new_files(directory, run, algorithm, random_data_gen):
    """`append --yes` should add unknown files without any prompting."""
    run(f"sign -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())

    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" not in f.read()

    run(f"append --yes {directory}")

    with catalog.path.open() as f:
        assert "new_file" in f.read()


def test_append_does_not_touch_modified_files(directory, run, algorithm, random_data_gen):
    """append only cares about unknown files; modified/missing entries must be left alone."""
    run(f"sign -a {algorithm} {directory}")
    catalog = algorithm.signature_file(directory)
    original_entries = {e.relpath: e.signature for e in catalog.entries()}

    modified_file = directory / "a" / "1"
    with modified_file.open("wb") as f:
        f.write(random_data_gen())

    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())

    run(f"append --yes {directory}")

    new_entries = {e.relpath: e.signature for e in catalog.entries()}

    assert new_entries["a/1"] == original_entries["a/1"], "append must not update a modified file"
    assert "new_file" in new_entries


def test_append_with_no_new_files_is_noop(directory, run, algorithm):
    run(f"sign -a {algorithm} {directory}")
    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        before = f.read()

    run(f"append --yes {directory}")

    with catalog.path.open() as f:
        after = f.read()

    assert before == after


def run_interactive_append(binary, directory, proceed_response="y"):
    """Run `append` interactively and answer the single bulk confirmation prompt.

    Unlike `update`, `append` must never show the per-file
    `[S]kip [U]pdate [D]irectory [A]ll` prompt -- it goes straight to a single
    "Proceed with updates?" confirmation listing every new file.
    """
    cmd = f"{binary} -vvv append ."
    child = pexpect.spawn(cmd, cwd=str(directory), timeout=30)

    try:
        response_idx = child.expect(
            [
                r"Proceed with updates\? \[y/N\]:",
                r"\[S\]kip \[U\]pdate \[D\]irectory \[A\]ll",
                r"Nothing to do\.",
                pexpect.EOF,
            ],
            timeout=10,
        )

        assert response_idx != 1, "append must not show the per-file update prompt"

        if response_idx == 0:
            child.send(proceed_response)

        child.expect(pexpect.EOF, timeout=10)
        child.wait()
        exit_code = child.exitstatus
        child.close()
        return exit_code
    except pexpect.TIMEOUT:
        child.kill(9)
        child.close()
        pytest.fail(f"Interactive append command timed out. Before: {child.before}")


def test_append_interactive_single_confirmation(directory, binary, algorithm, random_data_gen):
    subprocess.check_call(f"{binary} -vvv sign -a {algorithm} .", shell=True, cwd=directory)

    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())

    exit_code = run_interactive_append(binary, directory, "y")
    assert exit_code == 0

    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" in f.read()


def test_append_interactive_declined(directory, binary, algorithm, random_data_gen):
    subprocess.check_call(f"{binary} -vvv sign -a {algorithm} .", shell=True, cwd=directory)

    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())

    exit_code = run_interactive_append(binary, directory, "n")
    assert exit_code == 0

    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" not in f.read()
