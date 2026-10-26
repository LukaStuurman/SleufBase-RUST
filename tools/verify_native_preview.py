"""Inspect the complete native ZIP without extracting or executing its payload."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import struct
import zipfile

REQUIRED = {"sleufbase-rust.exe", "README.md", "Pillow-LICENSE.txt", "ezdxf-LICENSE.txt", "proj4rs-NOTICES.txt"}
REQUIRED |= {"assets/template_logos/" + name for name in ("apk.png", "bgm.png", "brabant_water.png", "enexis.png", "hanab.jpg", "hurkmans.png", "techbase_full.png", "wml.jpg")}

def verify(archive: Path, checksum: Path) -> dict:
    with archive.open("rb") as file:
        actual = hashlib.file_digest(file, "sha256").hexdigest()
    if checksum.read_text(encoding="ascii").strip() != actual + "  " + archive.name:
        raise ValueError("Archive checksum or filename differs")
    with zipfile.ZipFile(archive) as zipped:
        entries = zipped.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(name.casefold() for name in names)) or set(names) != REQUIRED | {"build-info.json"}:
            raise ValueError("Missing, extra or duplicate package entries")
        if sum(entry.file_size for entry in entries) > 2 * 1024**3:
            raise ValueError("Package exceeds the unpacked byte budget")
        for entry in entries:
            path = PurePosixPath(entry.filename)
            if path.is_absolute() or "\\" in entry.filename or any(part in (".", "..") or ":" in part for part in path.parts):
                raise ValueError("Unsafe package member path")
            if entry.flag_bits & 1 or (entry.external_attr >> 16) & 0o170000 not in (0, 0o100000):
                raise ValueError("Encrypted or non-regular package member")
        if zipped.getinfo("build-info.json").file_size > 1024 * 1024:
            raise ValueError("Package manifest is too large")
        manifest = json.loads(zipped.read("build-info.json"))
        if set(manifest) != {"format", "version", "application_version", "revision", "platform", "files"} or manifest["format"] != "sleufbase-rust-preview" or manifest["version"] != 1 or manifest["platform"] != "windows-x86_64":
            raise ValueError("Unknown package manifest")
        if not isinstance(manifest["application_version"], str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", manifest["application_version"]):
            raise ValueError("Invalid application version")
        if manifest["revision"] != "working-tree" and not re.fullmatch(r"[0-9a-f]{40}", str(manifest["revision"])):
            raise ValueError("Invalid build revision")
        records = manifest["files"]
        if not isinstance(records, list) or len(records) != len(REQUIRED) or {r.get("path") for r in records} != REQUIRED:
            raise ValueError("Incomplete manifest payload list")
        for record in records:
            if set(record) != {"path", "size", "sha256"} or type(record["size"]) is not int or record["size"] <= 0 or not re.fullmatch(r"[0-9a-f]{64}", str(record["sha256"])):
                raise ValueError("Invalid payload record")
            entry = zipped.getinfo(record["path"])
            if entry.file_size != record["size"]:
                raise ValueError("Payload size differs: " + record["path"])
            digest = hashlib.sha256()
            prefix = bytearray()
            with zipped.open(entry) as incoming:
                while chunk := incoming.read(1024 * 1024):
                    digest.update(chunk)
                    if record["path"] == "sleufbase-rust.exe" and len(prefix) < 1024 * 1024:
                        prefix.extend(chunk[:1024 * 1024 - len(prefix)])
            if digest.hexdigest() != record["sha256"]:
                raise ValueError("Payload hash differs: " + record["path"])
            if record["path"] == "sleufbase-rust.exe":
                if len(prefix) < 64 or prefix[:2] != b"MZ":
                    raise ValueError("Executable is not a Windows PE image")
                offset = struct.unpack_from("<I", prefix, 60)[0]
                if offset + 26 > len(prefix) or prefix[offset:offset+4] != b"PE\0\0" or struct.unpack_from("<H", prefix, offset+4)[0] != 0x8664 or struct.unpack_from("<H", prefix, offset+24)[0] != 0x20B:
                    raise ValueError("Executable is not Windows x86-64")
    return manifest

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("checksum", type=Path)
    args = parser.parse_args()
    manifest = verify(args.archive, args.checksum)
    print(len(manifest["files"]), "verified payload files; Windows x86-64;", manifest["revision"])

if __name__ == "__main__":
    main()
