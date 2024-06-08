# pylint: disable=redefined-outer-name
import json
import pytest
import subprocess
import pathlib
import random


_ROOT_DIR = pathlib.Path(__file__).absolute().parent.parent


def random_data():
    return bytes(random.randrange(256) for i in range(random.randrange(1000, 100_000)))


@pytest.fixture
def random_data_gen():
    return random_data


@pytest.fixture(scope="session")
def binary():
    subprocess.check_call("cargo build", shell=True, cwd=_ROOT_DIR)
    return _ROOT_DIR / "target" / "debug" / "ratify"


@pytest.fixture
def run(binary):
    def run_func(cmdline, **kw):
        cmdline = f"{binary} -vvv {cmdline}"
        print("*** Running", cmdline)
        return subprocess.check_output(cmdline, shell=True, **kw)

    return run_func


@pytest.fixture
def directory(tmpdir):
    tmpdir = pathlib.Path(tmpdir) / "dirname"
    for filename in ["a/1", "a/2", "b/3", "b/4", "c"]:
        path = tmpdir / filename
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("wb") as f:
            f.write(random_data())
    return tmpdir


class Entry:
    def __init__(self, root, relpath, signature):
        self.root = root
        self.relpath = relpath
        self.path = root / relpath
        self.signature = signature


class Sigfile:
    def __init__(self, path: pathlib.Path):
        self.path = path

    def entries(self) -> list[Entry]:
        with self.path.open() as f:
            entries = []
            for line in f:
                signature, relpath = line.strip().split(" *", 1)
                entries.append(Entry(self.path, relpath, signature))
            return entries

    def assert_all_files_contained(self, root, *, allow_unknown=False):
        assert not allow_unknown
        entries = self.entries()

        expected_entries = {p for p in root.rglob("*") if not p.is_dir()}
        expected_entries.remove(self.path)

        assert len(set(entries)) == len(entries), "Duplicates found"
        assert len(entries) == len(expected_entries)
        return entries

    def cfv_verify(self):
        subprocess.check_call("cfv", cwd=self.path.parent)


class Algorithm:
    def name(self):
        name = type(self).__name__
        suffix = "Algorithm"
        assert name.endswith(suffix)
        name = name[: -len(suffix)]
        return name

    def __str__(self):
        return self.name().lower()

    def signature_filename(self, directory: pathlib.Path):
        return f"{directory.name}.{self.name().lower()}"

    def signature_file(self, directory) -> Sigfile:
        path = directory / self.signature_filename(directory)
        return Sigfile(path)


class Sha1Algorithm(Algorithm):
    pass


class Md5Algorithm(Algorithm):
    pass


class Sha256Algorithm(Algorithm):
    pass


class Sha512Algorithm(Algorithm):
    pass


_ALL_ALGOS = (Sha1Algorithm, Md5Algorithm, Sha256Algorithm, Sha512Algorithm)


@pytest.fixture
def algos():
    return list(_ALL_ALGOS)


@pytest.fixture(params=_ALL_ALGOS)
def algorithm(request):
    return request.param()


@pytest.fixture
def cfv_sigfile(directory, algorithm):
    sigfile = algorithm.signature_file(directory)
    subprocess.check_call(f"cfv -C -t {algorithm} -rr .", cwd=directory, shell=True)
    assert sigfile.path.exists()
    return sigfile


class Report:
    def __init__(self, filename):
        self.filename = filename

    def load(self):
        with self.filename.open() as f:
            return json.load(f)


@pytest.fixture
def report(tmpdir):
    return Report(tmpdir / "report.json")
