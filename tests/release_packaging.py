"""Fast release publication guards; native executable smoke runs on release hosts."""

import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

SPEC = importlib.util.spec_from_file_location("release", Path(__file__).resolve().parents[1] / "scripts" / "release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
TAG = "v0.1.0"


class PublicationGuards(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="friendo-release-guards-")
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)
        for target in release.TARGETS:
            self.archive(target)

    def archive(self, target, extra=None, source=None, executable=True):
        prefix = f"friendo-{TAG}-{target}"
        files = {name: b"harmless packaging fixture" for name in release.package_files(target)}
        files["SOURCE.txt"] = (source if source is not None else release.source_record(TAG)).encode("utf-8")
        if extra:
            files[extra] = b"must never be shipped"
        path = self.output / release.archive_name(TAG, target)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path, "w") as archive:
                for name, data in files.items():
                    archive.writestr(f"{prefix}/{name}", data)
        else:
            with tarfile.open(path, "w:gz") as archive:
                for name, data in files.items():
                    member = tarfile.TarInfo(f"{prefix}/{name}")
                    member.size = len(data)
                    member.mode = 0o755 if executable and name == "friendo" else 0o644
                    archive.addfile(member, io.BytesIO(data))
        path.with_name(path.name + ".sha256").write_bytes(release.digest_line(path).encode("utf-8"))
        return path

    def test_complete_set_has_one_checksum_per_archive(self):
        release.finalize(self.output, TAG)
        lines = (self.output / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(lines), 4)
        self.assertEqual({line.split("  ")[1] for line in lines}, {release.archive_name(TAG, t) for t in release.TARGETS})

    def test_missing_platform_or_unexpected_asset_withholds_manifest(self):
        archive = self.output / release.archive_name(TAG, release.TARGETS[0])
        data = archive.read_bytes()
        archive.unlink()
        with self.assertRaisesRegex(ValueError, "exactly four"):
            release.finalize(self.output, TAG)
        archive.write_bytes(data)
        (self.output / "credentials.txt").write_bytes(b"fake canary")
        with self.assertRaisesRegex(ValueError, "exactly four"):
            release.finalize(self.output, TAG)
        self.assertFalse((self.output / "SHA256SUMS").exists())

    def test_wrong_checksum_withholds_manifest(self):
        archive = self.output / release.archive_name(TAG, release.TARGETS[0])
        archive.with_name(archive.name + ".sha256").write_bytes(b"0" * 64 + b"  wrong\n")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            release.finalize(self.output, TAG)
        self.assertFalse((self.output / "SHA256SUMS").exists())

    def test_unexpected_package_payload_withholds_manifest(self):
        for target in release.TARGETS:
            with self.subTest(target=target):
                self.archive(target, extra="friendo.toml")
                with self.assertRaisesRegex(ValueError, "unexpected package contents"):
                    release.finalize(self.output, TAG)
                self.archive(target)
        self.assertFalse((self.output / "SHA256SUMS").exists())

    def test_wrong_source_identity_withholds_manifest(self):
        for target in release.TARGETS:
            with self.subTest(target=target):
                self.archive(target, source="tag: v9.9.9\ncommit: unrelated\n")
                with self.assertRaisesRegex(ValueError, "source tag/commit"):
                    release.finalize(self.output, TAG)
                self.archive(target)
        self.assertFalse((self.output / "SHA256SUMS").exists())

    def test_tar_requires_executable_mode(self):
        self.archive(release.TARGETS[0], executable=False)
        with self.assertRaisesRegex(ValueError, "not executable"):
            release.finalize(self.output, TAG)

    def test_tag_must_match_cargo_version(self):
        with self.assertRaisesRegex(ValueError, "tag must match Cargo version"):
            release.validate("v9.9.9")


if __name__ == "__main__":
    unittest.main()
