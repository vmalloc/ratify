# pylint: disable=redefined-outer-name
import pytest
import subprocess
import pathlib


@pytest.fixture
def custom_catalog_paths(directory, algorithm, tmpdir):
    return {
        "subdirectory": directory / "custom_checksums" / f"my_catalog.{algorithm}",
        "relative": directory / f"custom.{algorithm}",
        "no_extension": directory / "my_catalog_no_ext",
        "absolute": pathlib.Path(tmpdir) / f"absolute_catalog.{algorithm}",
        "subdir_relative": directory / "catalogs" / f"backup.{algorithm}",
    }


def get_directory_contents(directory):
    return {p.relative_to(directory) for p in directory.rglob("*")}


def test_sign_with_custom_catalog_file(directory, run, algorithm, custom_catalog_paths):
    custom_catalog = custom_catalog_paths["subdirectory"]
    custom_catalog.parent.mkdir(exist_ok=True)

    contents_before = get_directory_contents(directory)

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)

    contents_after = get_directory_contents(directory)
    contents_after.discard(custom_catalog.relative_to(directory))

    assert contents_before == contents_after
    assert custom_catalog.exists()

    from conftest import Sigfile

    sigfile = Sigfile(custom_catalog)
    sigfile.assert_all_files_contained(directory, allow_unknown=False)


def test_sign_with_relative_catalog_file(
    directory, run, algorithm, custom_catalog_paths
):
    custom_catalog = custom_catalog_paths["relative"]

    contents_before = get_directory_contents(directory)

    run(f"sign -a {algorithm} --catalog-file custom.{algorithm} .", cwd=directory)

    contents_after = get_directory_contents(directory)
    contents_after.discard(custom_catalog.relative_to(directory))

    assert contents_before == contents_after
    assert custom_catalog.exists()

    from conftest import Sigfile

    sigfile = Sigfile(custom_catalog)
    sigfile.assert_all_files_contained(directory, allow_unknown=False)


def test_test_with_custom_catalog_file(directory, run, algorithm, custom_catalog_paths):
    custom_catalog = custom_catalog_paths["relative"]

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    run(f"test --catalog-file {custom_catalog} .", cwd=directory)


def test_test_with_custom_catalog_file_auto_detect_algo(
    directory, run, algorithm, custom_catalog_paths
):
    custom_catalog = custom_catalog_paths["relative"]

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    run(f"test --catalog-file {custom_catalog} .", cwd=directory)


def test_test_with_custom_catalog_file_no_extension_fails(
    directory, run, algorithm, custom_catalog_paths
):
    custom_catalog = custom_catalog_paths["no_extension"]

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)

    with pytest.raises(subprocess.CalledProcessError) as exc_info:
        run(f"test --catalog-file {custom_catalog} .", cwd=directory)

    assert exc_info.value.returncode != 0


def test_test_with_custom_catalog_file_no_extension_with_algo(
    directory, run, algorithm, custom_catalog_paths
):
    custom_catalog = custom_catalog_paths["no_extension"]

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)
    run(f"test -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)


def test_update_with_custom_catalog_file(
    directory, run, algorithm, random_data_gen, custom_catalog_paths
):
    custom_catalog = custom_catalog_paths["relative"]

    run(f"sign -a {algorithm} --catalog-file {custom_catalog} .", cwd=directory)

    test_file = directory / "a" / "1"
    with test_file.open("wb") as f:
        f.write(random_data_gen())

    run(f"update --confirm --catalog-file {custom_catalog} .", cwd=directory)
    run(f"test --catalog-file {custom_catalog} .", cwd=directory)


def test_sign_with_absolute_catalog_file(
    directory, run, algorithm, custom_catalog_paths
):
    abs_catalog = custom_catalog_paths["absolute"]

    contents_before = get_directory_contents(directory)

    run(f"sign -a {algorithm} --catalog-file {abs_catalog} .", cwd=directory)

    contents_after = get_directory_contents(directory)

    assert contents_before == contents_after
    assert abs_catalog.exists()

    from conftest import Sigfile

    sigfile = Sigfile(abs_catalog)
    entries = sigfile.entries()

    expected_entries = {p for p in directory.rglob("*") if not p.is_dir()}

    assert len(entries) == len(expected_entries)
    run(f"test --catalog-file {abs_catalog} .", cwd=directory)


def test_catalog_file_with_wrong_algorithm_extension(directory, run):
    wrong_catalog = directory / "wrong.md5"

    run(f"sign -a sha256 --catalog-file {wrong_catalog} .", cwd=directory)

    with pytest.raises(subprocess.CalledProcessError):
        run(f"test --catalog-file {wrong_catalog} .", cwd=directory)

    run(f"test -a sha256 --catalog-file {wrong_catalog} .", cwd=directory)


def test_catalog_file_in_subdirectory(directory, run, algorithm, custom_catalog_paths):
    subdir = directory / "catalogs"
    subdir.mkdir()
    catalog_file = custom_catalog_paths["subdir_relative"]

    contents_before = get_directory_contents(directory)

    run(
        f"sign -a {algorithm} --catalog-file catalogs/backup.{algorithm} .",
        cwd=directory,
    )

    contents_after = get_directory_contents(directory)
    contents_after.discard(catalog_file.relative_to(directory))

    assert contents_before == contents_after
    assert catalog_file.exists()

    run(f"test --catalog-file catalogs/backup.{algorithm} .", cwd=directory)
    run(f"update --confirm --catalog-file catalogs/backup.{algorithm} .", cwd=directory)
