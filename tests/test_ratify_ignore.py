# pylint: disable=redefined-outer-name


def _cataloged_relpaths(algorithm, directory):
    sigfile = algorithm.signature_file(directory)
    assert sigfile.path.exists()
    return {entry.relpath for entry in sigfile.entries()}


def test_ignore_bare_name_matches_any_depth(directory, run, algorithm):
    # A bare name matches a file of that name at any depth (gitignore-style).
    (directory / ".ratify-ignore").write_text("1\n")
    run(f"sign -a {algorithm} .", cwd=directory)

    relpaths = _cataloged_relpaths(algorithm, directory)
    assert "a/1" not in relpaths
    assert relpaths == {"a/2", "b/3", "b/4", "c"}


def test_ignore_leading_slash_is_root_relative(directory, run, algorithm):
    # A second file named 'c' deeper in the tree distinguishes anchoring from
    # any-depth matching.
    sub_c = directory / "sub" / "c"
    sub_c.parent.mkdir(parents=True, exist_ok=True)
    sub_c.write_bytes(b"deep-c")

    # "/c" is anchored to the directory root, NOT the filesystem root.
    (directory / ".ratify-ignore").write_text("/c\n")
    run(f"sign -a {algorithm} .", cwd=directory)

    relpaths = _cataloged_relpaths(algorithm, directory)
    assert "c" not in relpaths  # root-level c excluded
    assert "sub/c" in relpaths  # deeper c retained (anchored, not any-depth)


def test_ignore_glob(directory, run, algorithm):
    (directory / "x.log").write_bytes(b"log")
    (directory / "a" / "y.log").write_bytes(b"log")

    (directory / ".ratify-ignore").write_text("*.log\n")
    run(f"sign -a {algorithm} .", cwd=directory)

    relpaths = _cataloged_relpaths(algorithm, directory)
    assert not any(r.endswith(".log") for r in relpaths)
    assert relpaths == {"a/1", "a/2", "b/3", "b/4", "c"}


def test_ignore_file_is_self_excluded(directory, run, algorithm):
    (directory / ".ratify-ignore").write_text("c\n")
    run(f"sign -a {algorithm} .", cwd=directory)

    relpaths = _cataloged_relpaths(algorithm, directory)
    assert ".ratify-ignore" not in relpaths


def test_ignore_comments_and_blank_lines(directory, run, algorithm):
    (directory / ".ratify-ignore").write_text("# a note\n\nc\n")
    run(f"sign -a {algorithm} .", cwd=directory)

    relpaths = _cataloged_relpaths(algorithm, directory)
    assert "c" not in relpaths
    assert relpaths == {"a/1", "a/2", "b/3", "b/4"}


def test_verify_does_not_flag_ignored_files_as_unknown(
    directory, run, algorithm, report
):
    # Sign the clean tree first (no ignore file present yet).
    run(f"sign -a {algorithm} .", cwd=directory)

    # Introduce a stray file plus an ignore rule for it after signing.
    (directory / "junk.tmp").write_bytes(b"junk")
    (directory / ".ratify-ignore").write_text("junk.tmp\n")

    # test must succeed (exit 0) and report no failures: neither the stray file
    # nor the .ratify-ignore file itself should show up as unknown.
    run(
        f"test -a {algorithm} . --report json --report-filename {report.filename}",
        cwd=directory,
    )
    data = report.load()
    assert data["failed"] == []
