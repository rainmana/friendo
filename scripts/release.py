#!/usr/bin/env python3
"""Package and verify release binaries; Python is a build-time helper only."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
)


def validate(tag):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if tag != f"v{version}":
        raise ValueError(f"tag must match Cargo version: v{version}")
    if not (ROOT / "docs" / "releases" / f"{tag}.md").is_file():
        raise ValueError(f"missing release notes for {tag}")
    return version


def archive_name(tag, target):
    suffix = ".zip" if target.endswith("windows-msvc") else ".tar.gz"
    return f"friendo-{tag}-{target}{suffix}"


def source_record(tag):
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    repository = os.environ.get("GITHUB_REPOSITORY", "rainmana/friendo")
    return (
        f"tag: {tag}\ncommit: {commit}\n"
        f"source: https://github.com/{repository}/tree/{tag}\n"
        f"source archive: https://github.com/{repository}/archive/refs/tags/{tag}.tar.gz\n"
    )


def package_files(target):
    binary = "friendo.exe" if target.endswith("windows-msvc") else "friendo"
    return {binary, "LICENSE", "README.md", "friendo.example.toml", "SOURCE.txt"}


def check_archive(path, tag, target):
    prefix = f"friendo-{tag}-{target}"
    expected = {f"{prefix}/{name}" for name in package_files(target)}
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if len(names) != len(expected) or set(names) != expected:
                raise ValueError(f"unexpected package contents: {path.name}")
            source = archive.read(f"{prefix}/SOURCE.txt")
            if archive.testzip() is not None:
                raise ValueError(f"corrupt zip: {path.name}")
    else:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            files = [member for member in members if member.isfile()]
            if (
                len(files) != len(expected)
                or {member.name for member in files} != expected
                or any(not member.isfile() and not (member.isdir() and member.name == prefix) for member in members)
            ):
                raise ValueError(f"unexpected package contents: {path.name}")
            binary = archive.getmember(f"{prefix}/friendo")
            if not binary.mode & 0o111:
                raise ValueError("packaged binary is not executable")
            source = archive.extractfile(f"{prefix}/SOURCE.txt").read()
    if source.decode() != source_record(tag):
        raise ValueError(f"source tag/commit does not match checkout: {path.name}")


def smoke(path, tag, target, version):
    with tempfile.TemporaryDirectory(prefix="friendo-release-smoke-") as directory:
        work = Path(directory)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                archive.extractall(work)
        else:
            with tarfile.open(path, "r:gz") as archive:
                archive.extractall(work, filter="data")
        binary_name = "friendo.exe" if target.endswith("windows-msvc") else "friendo"
        binary = work / f"friendo-{tag}-{target}" / binary_name

        def run(*arguments):
            return subprocess.run(
                [str(binary), "--data-dir", str(work / "runs"), "--effects", "off", "--no-color", *arguments],
                cwd=work, check=True, capture_output=True, text=True, encoding="utf-8", timeout=60,
            ).stdout

        if run("--version").strip() != f"friendo {version}":
            raise ValueError("packaged binary version mismatch")
        doctor = run("doctor")
        inspector = json.loads(doctor[doctor.index("{"):])
        if "No providers configured" not in doctor or inspector["fields"]["autonomy"]["value"] != "normal":
            raise ValueError("packaged binary did not start with offline NORMAL defaults")
        if (work / "runs").exists():
            raise ValueError("doctor unexpectedly created a run")
        summary = json.loads(run("demo", "--headless"))
        if summary["synthetic"] is not True or summary["revision"] != 2:
            raise ValueError("packaged demo did not complete the synthetic repair")
        before = {p.relative_to(work): p.read_bytes() for p in (work / "runs").rglob("*") if p.is_file()}
        state = json.loads(run("replay", summary["run_id"], "--headless"))
        after = {p.relative_to(work): p.read_bytes() for p in (work / "runs").rglob("*") if p.is_file()}
        if state["restored"] is not True or len(state["constraints"]) != 1 or before != after:
            raise ValueError("packaged replay failed or modified retained evidence")


def digest_line(path):
    return f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"


def package(output, tag, target, version):
    binary = "friendo.exe" if target.endswith("windows-msvc") else "friendo"
    executable = ROOT / "target" / target / "release" / binary
    output.mkdir(parents=True, exist_ok=True)
    path = output / archive_name(tag, target)
    if path.exists() or path.with_name(path.name + ".sha256").exists():
        raise ValueError(f"refusing to overwrite existing release asset: {path}")
    with tempfile.TemporaryDirectory(prefix="friendo-release-package-") as directory:
        stage = Path(directory) / f"friendo-{tag}-{target}"
        stage.mkdir()
        shutil.copy2(executable, stage / binary)
        for name in ("LICENSE", "friendo.example.toml"):
            shutil.copy2(ROOT / name, stage / name)
        repository = os.environ.get("GITHUB_REPOSITORY", "rainmana/friendo")
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        readme = readme.replace(
            "](docs/", f"](https://github.com/{repository}/blob/{tag}/docs/"
        )
        (stage / "README.md").write_bytes(readme.encode("utf-8"))
        (stage / "SOURCE.txt").write_bytes(source_record(tag).encode("utf-8"))
        if path.suffix == ".zip":
            with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
                for child in sorted(stage.iterdir()):
                    archive.write(child, f"{stage.name}/{child.name}")
        else:
            def attributes(member):
                member.uid = member.gid = 0
                member.uname = member.gname = ""
                member.mode = 0o755 if member.isdir() or member.name.endswith("/friendo") else 0o644
                return member

            with tarfile.open(path, "w:gz") as archive:
                archive.add(stage, arcname=stage.name, filter=attributes)
    check_archive(path, tag, target)
    smoke(path, tag, target, version)
    path.with_name(path.name + ".sha256").write_bytes(digest_line(path).encode("utf-8"))
    print(json.dumps({"archive": path.name, "packaged_binary_smoke": "passed"}))


def finalize(output, tag):
    names = {archive_name(tag, target) for target in TARGETS}
    expected = names | {f"{name}.sha256" for name in names}
    if {path.name for path in output.iterdir()} != expected:
        raise ValueError("release requires exactly four archives and their four checksum files")
    lines = []
    for target in TARGETS:
        path = output / archive_name(tag, target)
        check_archive(path, tag, target)
        line = digest_line(path)
        if path.with_name(path.name + ".sha256").read_text() != line:
            raise ValueError(f"checksum mismatch: {path.name}")
        lines.append(line)
    (output / "SHA256SUMS").write_bytes("".join(sorted(lines)).encode("utf-8"))
    print("Verified all four platform packages and wrote SHA256SUMS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "package", "finalize"))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "release-assets")
    args = parser.parse_args()
    version = validate(args.tag)
    if args.command == "package":
        if not args.target:
            parser.error("package requires --target")
        package(args.output.resolve(), args.tag, args.target, version)
    elif args.command == "finalize":
        finalize(args.output.resolve(), args.tag)
    else:
        print(f"Validated {args.tag}")


if __name__ == "__main__":
    main()
