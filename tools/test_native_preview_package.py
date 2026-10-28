"""Adversarial package tests; the small PE fixture is structural, never executed."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
from package_native_preview import build, ROOT, NAME
from verify_native_preview import verify

class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="sleufbase-package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "structure-only.exe"
        pe = bytearray(4096)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 128)
        pe[128:132] = b"PE\0\0"
        struct.pack_into("<H", pe, 132, 0x8664)
        struct.pack_into("<H", pe, 152, 0x20B)
        self.binary.write_bytes(pe)

    def package(self, name="package"):
        return build(self.binary, self.root / name, "working-tree", ROOT / "rust/third_party", ROOT / "assets/template_logos")

    def checksum(self, archive):
        checksum = archive.with_suffix(".sha256")
        checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n", encoding="ascii")
        return checksum

    def modified(self, mutate):
        archive = self.package()
        with zipfile.ZipFile(archive) as source:
            members = {name: source.read(name) for name in source.namelist()}
        mutate(members)
        altered = self.root / "altered.zip"
        with zipfile.ZipFile(altered, "x", compression=zipfile.ZIP_DEFLATED) as target:
            for name, content in members.items():
                target.writestr(name, content)
        return altered, self.checksum(altered)

    def test_full_bundle_and_identical_rebuild(self):
        first = self.package("first")
        second = self.package("second")
        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(len(verify(first, first.with_suffix(".sha256"))["files"]), 15)
        with zipfile.ZipFile(first) as archive:
            self.assertEqual(archive.read("sleufbase-rust.exe"), self.binary.read_bytes())
            self.assertEqual(archive.read("assets/template_logos/techbase_full.png"), (ROOT / "assets/template_logos/techbase_full.png").read_bytes())

    def test_missing_required_assets_never_publish_a_package(self):
        destination = self.root / "absent"
        with self.assertRaises(ValueError):
            build(self.binary, destination, "working-tree", self.root / "missing-support", ROOT / "assets/template_logos")
        self.assertFalse(destination.exists())

    def test_existing_directory_is_never_overwritten(self):
        destination = self.root / "kept"
        destination.mkdir()
        marker = destination / "prior.txt"
        marker.write_bytes(b"keep earlier release")
        with self.assertRaises(ValueError):
            self.package("kept")
        self.assertEqual(marker.read_bytes(), b"keep earlier release")

    def test_changed_payload_fails_even_with_a_new_valid_archive_checksum(self):
        archive, checksum = self.modified(lambda members: members.__setitem__("assets/template_logos/wml.jpg", b"changed"))
        with self.assertRaisesRegex(ValueError, "size differs|hash differs"):
            verify(archive, checksum)

    def test_missing_extra_and_case_duplicate_members_are_rejected(self):
        mutations = [lambda m: m.pop("assets/template_logos/wml.jpg"), lambda m: m.__setitem__("../outside.txt", b"outside"), lambda m: m.__setitem__("README.MD", b"duplicate")]
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                archive = self.package("base" + str(index))
                with zipfile.ZipFile(archive) as source:
                    members = {name: source.read(name) for name in source.namelist()}
                mutate(members)
                altered = self.root / (str(index) + ".zip")
                with zipfile.ZipFile(altered, "x") as target:
                    for name, content in members.items():
                        target.writestr(name, content)
                with self.assertRaises(ValueError):
                    verify(altered, self.checksum(altered))

    def test_internally_consistent_wrong_architecture_is_rejected(self):
        def mutate(members):
            executable = bytearray(members["sleufbase-rust.exe"])
            struct.pack_into("<H", executable, 132, 0x14C)
            members["sleufbase-rust.exe"] = bytes(executable)
            manifest = json.loads(members["build-info.json"])
            for record in manifest["files"]:
                if record["path"] == "sleufbase-rust.exe":
                    record["sha256"] = hashlib.sha256(executable).hexdigest()
            members["build-info.json"] = json.dumps(manifest).encode()
        archive, checksum = self.modified(mutate)
        with self.assertRaisesRegex(ValueError, "x86-64"):
            verify(archive, checksum)

if __name__ == "__main__":
    unittest.main()
