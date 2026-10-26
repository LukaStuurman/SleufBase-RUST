"""Build a complete, immutable native preview bundle in a fresh directory."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
NAME = "SleufBase-Rust-portable-preview"
LOGOS = ("apk.png", "bgm.png", "brabant_water.png", "enexis.png", "hanab.jpg", "hurkmans.png", "techbase_full.png", "wml.jpg")
NOTICES = ("Pillow-LICENSE.txt", "ezdxf-LICENSE.txt", "proj4rs-NOTICES.txt")

def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()

def build(binary: Path, output: Path, revision: str, support: Path, logos: Path) -> Path:
    output = output.resolve()
    if output.exists():
        raise ValueError("Output directory already exists; choose a new directory")
    if revision != "working-tree" and (len(revision) != 40 or any(c not in "0123456789abcdef" for c in revision)):
        raise ValueError("Revision must be the full commit SHA or working-tree")
    files = {"sleufbase-rust.exe": binary, "README.md": ROOT / "rust/PORTABLE.md"}
    files.update({name: support / name for name in NOTICES})
    files.update({"assets/template_logos/" + name: logos / name for name in LOGOS})
    for relative, source in files.items():
        if source.is_symlink() or not source.is_file() or source.stat().st_size == 0:
            raise ValueError("Missing, linked or empty required payload: " + relative)
    version = tomllib.loads((ROOT / "rust/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".sleufbase-preview-", dir=output.parent) as temporary:
        staging = Path(temporary)
        bundle = staging / NAME
        bundle.mkdir()
        records = []
        for relative, source in sorted(files.items()):
            target = bundle / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            records.append({"path": relative, "size": target.stat().st_size, "sha256": digest(target)})
        manifest = {"format": "sleufbase-rust-preview", "version": 1, "application_version": version,
                    "revision": revision, "platform": "windows-x86_64", "files": records}
        (bundle / "build-info.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
        archive = staging / (NAME + ".zip")
        with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as zipped:
            for source in sorted(p for p in bundle.rglob("*") if p.is_file()):
                info = zipfile.ZipInfo(source.relative_to(bundle).as_posix(), date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                with source.open("rb") as incoming, zipped.open(info, "w", force_zip64=True) as outgoing:
                    shutil.copyfileobj(incoming, outgoing, 1024 * 1024)
        checksum = staging / (NAME + ".sha256")
        checksum.write_text(digest(archive) + "  " + archive.name + "\n", encoding="ascii")
        from verify_native_preview import verify
        verify(archive, checksum)
        # Publish the entire verified folder at once. On Windows rename refuses
        # an existing destination; an earlier package can never be overwritten.
        os.rename(staging, output)
    return output / archive.name

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--support", type=Path, default=ROOT / "rust/third_party")
    parser.add_argument("--logos", type=Path, default=ROOT / "assets/template_logos")
    args = parser.parse_args()
    print(build(args.binary, args.output, args.revision, args.support, args.logos))

if __name__ == "__main__":
    main()
