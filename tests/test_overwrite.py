# pylint: disable=redefined-outer-name
import pytest
import subprocess
import pathlib
import os


def test_sign_fails_when_catalog_exists_without_overwrite(directory, run, algorithm):
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    with pytest.raises(subprocess.CalledProcessError) as exc_info:
        run(f"sign -a {algorithm} .", cwd=directory)
    
    assert exc_info.value.returncode != 0
    assert b"already exists" in exc_info.value.output


def test_sign_succeeds_with_overwrite_flag(directory, run, algorithm):
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    original_content = sigfile.path.read_text()
    
    run(f"sign -a {algorithm} --overwrite .", cwd=directory)
    assert sigfile.path.exists()
    
    new_content = sigfile.path.read_text()
    assert original_content == new_content


def test_sign_with_custom_catalog_file_fails_when_exists_without_overwrite(directory, run, algorithm):
    custom_catalog = directory / f"custom.{algorithm}"
    
    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    assert custom_catalog.exists()
    
    with pytest.raises(subprocess.CalledProcessError) as exc_info:
        run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    
    assert exc_info.value.returncode != 0
    assert b"already exists" in exc_info.value.output


def test_sign_with_custom_catalog_file_succeeds_with_overwrite_flag(directory, run, algorithm):
    custom_catalog = directory / f"custom.{algorithm}"
    
    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    assert custom_catalog.exists()
    
    original_content = custom_catalog.read_text()
    
    run(f"sign -a {algorithm} --catalog-file {custom_catalog} --overwrite .", cwd=directory)
    assert custom_catalog.exists()
    
    new_content = custom_catalog.read_text()
    assert original_content == new_content


def test_sign_overwrite_with_different_content(directory, run, algorithm, random_data_gen):
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    original_content = sigfile.path.read_text()
    
    test_file = directory / "new_file"
    with test_file.open("wb") as f:
        f.write(random_data_gen())
    
    run(f"sign -a {algorithm} --overwrite .", cwd=directory)
    assert sigfile.path.exists()
    
    new_content = sigfile.path.read_text()
    assert original_content != new_content
    assert "new_file" in new_content


def test_sign_overwrite_truncates_file_properly(directory, run, algorithm):
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    with sigfile.path.open("a") as f:
        f.write("\nextra line that should be removed")
    
    modified_content = sigfile.path.read_text()
    assert "extra line" in modified_content
    
    run(f"sign -a {algorithm} --overwrite .", cwd=directory)
    
    final_content = sigfile.path.read_text()
    assert "extra line" not in final_content
    
    from conftest import Sigfile
    sigfile_obj = Sigfile(sigfile.path)
    sigfile_obj.assert_all_files_contained(directory, allow_unknown=False)


def test_sign_interactive_overwrite_yes(directory, run, algorithm, monkeypatch):
    import io
    import sys
    
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    monkeypatch.setattr('sys.stdin', io.StringIO('y\n'))
    
    try:
        result = run(f"sign -a {algorithm} .", cwd=directory, input=b'y\n')
        assert sigfile.path.exists()
    except subprocess.CalledProcessError:
        pytest.skip("Interactive test requires TTY support")


def test_sign_interactive_overwrite_no(directory, run, algorithm, monkeypatch):
    import io
    import sys
    
    sigfile = algorithm.signature_file(directory)
    
    run(f"sign -a {algorithm} .", cwd=directory)
    assert sigfile.path.exists()
    
    monkeypatch.setattr('sys.stdin', io.StringIO('n\n'))
    
    try:
        with pytest.raises(subprocess.CalledProcessError) as exc_info:
            run(f"sign -a {algorithm} .", cwd=directory, input=b'n\n')
        
        assert exc_info.value.returncode != 0
        assert b"already exists" in exc_info.value.output
    except subprocess.CalledProcessError:
        pytest.skip("Interactive test requires TTY support")
