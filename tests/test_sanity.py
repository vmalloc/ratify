# pylint: disable=redefined-outer-name
import pytest
import subprocess


def test_create(directory, run, algorithm):
    run(f"create -a {algorithm} .", cwd=directory)
    sigfile = algorithm.signature_file(directory)
    assert sigfile.path.exists()

    sigfile.assert_all_files_contained(directory, allow_unknown=False)
    sigfile.verify()


def test_cfv_verify(directory, algorithm, run, cfv_sigfile):
    run(f"verify -a {algorithm} .", cwd=directory)
    run(f"verify -a {algorithm} .", cwd=directory)  # same
    run(f"verify -a {algorithm}", cwd=directory)  # also same


def test_cfv_verify_no_specifying_algo(directory, algorithm, run, cfv_sigfile):
    run("verify .", cwd=directory)


def test_list_algos(run, algos):
    output = run("list-algos")
    assert {s.decode("utf-8") for s in output.splitlines()} == {
        str(algo()) for algo in algos
    }
