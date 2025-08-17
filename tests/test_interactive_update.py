# pylint: disable=redefined-outer-name
import pytest
import subprocess
import pathlib
import time

try:
    import pexpect
except ImportError:
    pytest.skip("pexpect is required for interactive update tests", allow_module_level=True)


def run_interactive_update(binary, directory, file_response, proceed_response='y'):
    """Helper function to run interactive update command with pexpect.

    Args:
        binary: Path to the ratify binary
        directory: Directory to run the command in
        file_response: Response for file update choice ('u', 's', 'd', 'a')
        proceed_response: Response for proceed confirmation ('y', 'n')

    Returns:
        Exit code of the update command

    Raises:
        pytest.fail: If the command times out or doesn't show expected prompt
    """
    cmd = f"{binary} -vvv update ."
    child = pexpect.spawn(cmd, cwd=str(directory), timeout=30)

    try:
        # First expect the file update choice question
        child.expect(r"\[S\]kip \[U\]pdate \[D\]irectory \[A\]ll \(default: Skip\):", timeout=10)

        # Send the file response
        child.send(file_response)

        # If we chose to skip, ratify might say "Nothing to do." and exit
        # If we chose to update, ratify will ask for confirmation
        response_idx = child.expect([
            r"Proceed with updates\? \[y/N\]:",
            r"Nothing to do\.",
            pexpect.EOF
        ], timeout=10)

        if response_idx == 0:
            # Got the proceed confirmation question
            child.send(proceed_response)
            child.expect(pexpect.EOF, timeout=10)
        elif response_idx == 1:
            # Got "Nothing to do." message, command will exit
            child.expect(pexpect.EOF, timeout=10)
        # response_idx == 2 means we got EOF directly

        # Wait for the process to actually exit
        child.wait()
        exit_code = child.exitstatus
        child.close()

        return exit_code

    except pexpect.TIMEOUT as e:
        print(f"TIMEOUT: {e}")
        print(f"Before: {child.before}")
        print(f"After: {child.after}")
        child.kill(9)
        child.close()
        pytest.fail(f"Interactive update command timed out. Before: {child.before}, After: {child.after}")

    except pexpect.EOF as e:
        print(f"EOF: {e}")
        print(f"Before: {child.before}")
        child.close()
        pytest.fail(f"Interactive update command ended unexpectedly. Before: {child.before}")


def test_interactive_update_single_file(directory, binary, algorithm, random_data_gen):
    """Test interactive update functionality with pexpect.

    This test:
    1. Creates a signature file for a directory
    2. Modifies a single file in the directory
    3. Runs the update command interactively
    4. Answers 'update' (u) for the file choice
    5. Answers 'yes' (y) to proceed with updates
    6. Verifies that ratify test works afterwards
    """
    # Step 1: Create initial signature
    subprocess.check_call(
        f"{binary} -vvv sign -a {algorithm} .",
        shell=True,
        cwd=directory
    )

    sigfile = algorithm.signature_file(directory)
    assert sigfile.path.exists()

    # Step 2: Modify a single file
    target_file = directory / "a" / "1"
    assert target_file.exists()

    with target_file.open("wb") as f:
        f.write(random_data_gen())

    # Step 3-5: Run update command interactively and answer 'update' then 'yes'
    exit_code = run_interactive_update(binary, directory, 'u', 'y')
    assert exit_code == 0, f"Update command failed with exit code {exit_code}"

    # Step 6: Verify that ratify test works afterwards (no errors)
    result = subprocess.run(
        f"{binary} -vvv test .",
        shell=True,
        cwd=directory,
        capture_output=True
    )

    assert result.returncode == 0, f"Test command failed after update: {result.stderr.decode()}"


def test_interactive_update_skip_file(directory, binary, algorithm, random_data_gen):
    """Test interactive update functionality when choosing to skip files.

    This test verifies that when we choose to skip a modified file,
    the signature is not updated and test fails afterwards.
    """
    # Step 1: Create initial signature
    subprocess.check_call(
        f"{binary} -vvv sign -a {algorithm} .",
        shell=True,
        cwd=directory
    )

    sigfile = algorithm.signature_file(directory)
    assert sigfile.path.exists()

    # Step 2: Modify a single file
    target_file = directory / "a" / "1"
    assert target_file.exists()

    with target_file.open("wb") as f:
        f.write(random_data_gen())

    # Step 3-5: Run update command interactively and answer 'skip' then 'yes'
    exit_code = run_interactive_update(binary, directory, 's', 'y')
    assert exit_code == 0, f"Update command failed with exit code {exit_code}"

    # Step 6: Verify that ratify test fails afterwards (because file was skipped)
    result = subprocess.run(
        f"{binary} -vvv test .",
        shell=True,
        cwd=directory,
        capture_output=True
    )

    assert result.returncode != 0, "Test command should have failed after skipping the modified file"


def test_unknown_files_warning(directory, run, algorithm, random_data_gen, report):
    """Test that unknown files are detected and reported properly.

    This test was migrated from test_append.py and verifies that
    unknown files trigger appropriate warnings and reports.
    """
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


def test_update_with_confirm_flag(directory, run, algorithm, random_data_gen):
    """Test update command with --confirm flag (replaces old append functionality).

    This test was migrated from test_append.py and verifies that
    'update --confirm' automatically adds new files to the catalog
    without user interaction.
    """
    run(f"sign -a {algorithm} {directory}")
    with (directory / "new_file").open("wb") as f:
        f.write(random_data_gen())
    catalog = algorithm.signature_file(directory)
    with catalog.path.open() as f:
        assert "new_file" not in f.read()
    run(f"update --confirm {directory}")
    with catalog.path.open() as f:
        assert "new_file" in f.read()