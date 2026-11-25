"""Verify the public Git tree, Cargo package inventory, and release archives."""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath

PROJECT = re.compile(rb"math[-_ ](?:attack|invasion)", re.I)
SECRETS = re.compile(
    rb"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----"
    rb"|gh[pousr]_[a-zA-Z0-9]{30,}|github_pat_[a-zA-Z0-9_]{30,}"
    rb"|AKIA[A-Z0-9]{16}|xox[baprs]-[a-zA-Z0-9-]{20,}"
)
LOCAL_DIRECTORIES = {
    ".agents", ".aws", ".build", ".codex", ".coverage", ".godot", ".idea",
    ".tmp", ".tools", ".verification", ".llm-lint", "node_modules", "target",
}
LOCAL_SUFFIXES = {
    ".db", ".exe", ".log", ".pem", ".pfx", ".profdata", ".profraw",
    ".sqlite", ".sqlite3", ".zip",
}
TARGETS = {
    "x86_64-pc-windows-msvc": ".zip",
    "x86_64-unknown-linux-gnu": ".tar.gz",
    "x86_64-apple-darwin": ".tar.gz",
    "aarch64-apple-darwin": ".tar.gz",
}


def output(*command: str) -> bytes:
    return subprocess.check_output(command)


def check_content(label: str, data: bytes) -> None:
    if PROJECT.search(data) or SECRETS.search(data):
        raise SystemExit(f"Private source marker or credential pattern in {label}")


def check_tree(revision: str, blobs: set[str]) -> list[str]:
    files = []
    for entry in output("git", "ls-tree", "-rz", revision).split(b"\0"):
        if not entry:
            continue
        header, raw_path = entry.split(b"\t", 1)
        mode, kind, oid = header.decode().split()
        name = raw_path.decode()
        path = PurePosixPath(name)
        if (
            kind != "blob"
            or mode not in {"100644", "100755"}
            or any(part.lower() in LOCAL_DIRECTORIES for part in path.parts)
            or path.name.lower().startswith((".env", ".codex-"))
            or path.suffix.lower() in LOCAL_SUFFIXES
        ):
            raise SystemExit(f"Unexpected public source entry {revision}:{name}")
        check_content(f"{revision}:{name}", raw_path)
        if oid not in blobs:
            check_content(f"{revision}:{name}", output("git", "cat-file", "blob", oid))
            blobs.add(oid)
        files.append(name)
    return files


def check_package(files: list[str]) -> None:
    manifest = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))
    patterns = manifest["package"]["include"]
    if any(not pattern.startswith("/") for pattern in patterns):
        raise SystemExit("Cargo package patterns must be anchored to the repository root")
    expected = {
        name
        for name in files
        if any(
            f"/{name}".startswith(pattern[:-2]) if pattern.endswith("/**")
            else f"/{name}" == pattern
            for pattern in patterns
        )
    }
    expected.update({"Cargo.toml.orig", ".cargo_vcs_info.json"})
    actual = {
        name.replace("\\", "/")
        for name in output("cargo", "package", "--locked", "--list").decode().splitlines()
    }
    if actual != expected:
        raise SystemExit(
            f"Unexpected package inventory. Extra: {sorted(actual - expected)}. "
            f"Missing: {sorted(expected - actual)}."
        )
    print(f"Verified {len(actual)} Cargo package entries.")


def check_archive(path: Path, windows: bool) -> None:
    names = {
        "godot-llm-lint.exe" if windows else "godot-llm-lint",
        "LICENSE", "THIRD_PARTY_NOTICES.txt", "RUST_STANDARD_LIBRARY_NOTICES.html",
    }
    if windows:
        with zipfile.ZipFile(path) as archive:
            info = archive.infolist()
            if len(info) != len(names) or {item.filename for item in info} != names:
                raise SystemExit(f"Unexpected files in {path.name}")
            for item in info:
                if item.is_dir() or (item.external_attr >> 16) & 0o170000 == 0o120000:
                    raise SystemExit(f"Unexpected linked or non-file entry in {path.name}")
                check_content(f"{path.name}:{item.filename}", archive.read(item))
    else:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            if len(members) != len(names) or {item.name for item in members} != names:
                raise SystemExit(f"Unexpected files in {path.name}")
            for member in members:
                stream = archive.extractfile(member) if member.isfile() else None
                if stream is None:
                    raise SystemExit(f"Unexpected linked or non-file entry in {path.name}")
                with stream:
                    check_content(f"{path.name}:{member.name}", stream.read())


def check_assets(directory: Path, tag: str) -> None:
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise SystemExit("Expected a version tag such as v0.5.0")
    archives = {f"godot-llm-lint-{tag}-{target}{ext}" for target, ext in TARGETS.items()}
    expected = archives | {name + ".sha256" for name in archives}
    if {path.name for path in directory.iterdir()} != expected:
        raise SystemExit("Expected exactly four platform archives and their checksums")
    for name in sorted(archives):
        path = directory / name
        digest = hashlib.file_digest(path.open("rb"), "sha256").hexdigest()
        if (directory / f"{name}.sha256").read_text().strip() != f"{digest}  {name}":
            raise SystemExit(f"Checksum mismatch for {name}")
        check_archive(path, path.suffix == ".zip")
    print(f"Verified the four platform archives for {tag}.")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", action="store_true", help="also inspect all reachable public commits")
    parser.add_argument("--assets", nargs=2, metavar=("DIRECTORY", "TAG"))
    args = parser.parse_args()
    if args.assets:
        check_assets(Path(args.assets[0]), args.assets[1])
        return

    blobs: set[str] = set()
    files = check_tree("HEAD", blobs)
    if args.history:
        for revision in output("git", "rev-list", "--all").decode().splitlines():
            check_tree(revision, blobs)
        print(f"Verified {len(blobs)} unique public history blobs.")
    check_package(files)


if __name__ == "__main__":
    main()
