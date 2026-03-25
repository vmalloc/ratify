# pylint: disable=redefined-outer-name
import pytest
import subprocess


def test_create(directory, run, algorithm):
    run(f"sign -a {algorithm} .", cwd=directory)
    sigfile = algorithm.signature_file(directory)
    assert sigfile.path.exists()

    sigfile.assert_all_files_contained(directory, allow_unknown=False)
    sigfile.cfv_verify()


def test_cfv_verify(directory, algorithm, run, cfv_sigfile):
    run(f"test -a {algorithm} .", cwd=directory)
    run(f"test -a {algorithm} .", cwd=directory)  # same
    run(f"test -a {algorithm}", cwd=directory)  # also same


def test_cfv_verify_no_specifying_algo(directory, algorithm, run, cfv_sigfile):
    run("test .", cwd=directory)


def test_create_uses_ratify_prefix(directory, run, algorithm):
    """Test that sign creates a ratify-catalog.<algo> file by default"""
    run(f"sign -a {algorithm} .", cwd=directory)
    expected_path = directory / f"ratify-catalog.{str(algorithm)}"
    assert expected_path.exists()
    # Ensure the old dirname-based name is NOT created
    old_path = directory / f"{directory.name}.{str(algorithm)}"
    assert not old_path.exists()


def test_detect_legacy_dirname_catalog(directory, run, algorithm):
    """Test that ratify can detect and verify a legacy <dirname>.<algo> catalog file"""
    # Create catalog with the legacy naming by using --catalog-file
    legacy_name = f"{directory.name}.{str(algorithm)}"
    run(f"sign -a {algorithm} --catalog-file {legacy_name} .", cwd=directory)
    assert (directory / legacy_name).exists()

    # Verify that ratify auto-detects the legacy-named file without --catalog-file
    run(f"test .", cwd=directory)


def test_list_algos(run, algos):
    output = run("list-algos")
    assert {s.decode("utf-8") for s in output.splitlines()} == {
        str(algo()) for algo in algos
    }
