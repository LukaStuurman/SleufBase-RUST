"""Prepare the pinned, offline Potree runtime used by the native Windows app.

This is a build/packaging step, never an application-startup downloader. The
official release archives are checked before extraction. Only the viewer's
build/potree, libs and license files, plus the converter and its DLLs/licenses,
are packaged; example point clouds and the converter's duplicate web viewer
are deliberately excluded. --offline accepts only already cached archives.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import struct
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE_MANIFEST = ROOT / "tools/pointcloud-runtime-manifest.json"
NOTICE = ROOT / "rust/third_party/Potree-NOTICES.txt"
DEFAULT_OUTPUT = ROOT / "rust/assets/pointcloud-runtime"
REQUIRED = (
    "build/potree/potree.js",
    "build/potree/potree.css",
    "build/potree/LICENSE",
    "build/potree/workers/2.0/DecoderWorker.js",
    "build/potree/workers/2.0/DecoderWorker_brotli.js",
    "libs/three.js/build/three.module.js",
    "libs/jquery/jquery-3.1.1.min.js",
    "libs/other/BinaryHeap.js",
    "libs/tween/tween.min.js",
    "converter/PotreeConverter.exe",
    "converter/laszip.dll",
    "converter/licenses/license_potree_converter.txt",
    "converter/licenses/license_laszip.txt",
    "LICENSE",
    "Potree-NOTICES.txt",
)
MSVC_REQUIRED = ("msvcp140.dll", "msvcp140_atomic_wait.dll", "vcruntime140.dll", "vcruntime140_1.dll")
WINDOWS_REQUIRED = tuple("converter/" + name for name in MSVC_REQUIRED) + (
    "converter/licenses/Microsoft-VCRuntime-NOTICES.txt",
    "converter/licenses/Microsoft-VCRuntime-PROVENANCE.json",
)


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def valid_archive(path: Path, source: dict) -> bool:
    return (path.is_file() and not path.is_symlink()
            and path.stat().st_size == source["size"]
            and digest(path) == source["sha256"])


def obtain_archive(source: dict, cache: Path, offline: bool) -> Path:
    path = cache / source["filename"]
    if path.exists():
        if not valid_archive(path, source):
            raise ValueError(f"Cached archive has the wrong size or SHA256: {path}. "
                             "Remove that archive and run this preparation step again.")
        return path
    if offline:
        raise ValueError(f"Offline archive is missing: {path}")
    cache.mkdir(parents=True, exist_ok=True)
    descriptor, partial_name = tempfile.mkstemp(prefix=path.name + ".", suffix=".part", dir=cache)
    partial = Path(partial_name)
    try:
        with os.fdopen(descriptor, "wb") as outgoing:
            request = urllib.request.Request(source["url"], headers={
                "User-Agent": "SleufBase-build/pointcloud-runtime",
                "Accept": "application/octet-stream",
            })
            with urllib.request.urlopen(request, timeout=45) as incoming:
                received = 0
                while block := incoming.read(1024 * 1024):
                    received += len(block)
                    if received > source["size"]:
                        raise ValueError(f"Archive exceeds its pinned size: {source['filename']}")
                    outgoing.write(block)
        if not valid_archive(partial, source):
            raise ValueError(f"Downloaded archive has the wrong size or SHA256: {source['filename']}")
        os.replace(partial, path)
    finally:
        partial.unlink(missing_ok=True)
    return path


def extraction_path(member: zipfile.ZipInfo, source: dict) -> PurePosixPath | None:
    name = PurePosixPath(member.filename)
    if (name.is_absolute() or ".." in name.parts or "\\" in member.filename
            or any(":" in part for part in name.parts)
            or stat.S_ISLNK(member.external_attr >> 16)):
        raise ValueError(f"Unsafe archive member: {member.filename}")
    if not name.parts or name.parts[0] != source["archive_root"]:
        raise ValueError(f"Unexpected release archive root: {member.filename}")
    relative = PurePosixPath(*name.parts[1:])
    if member.is_dir() or not relative.parts:
        return None
    if source["id"] == "potree":
        if (relative.parts[:2] == ("build", "potree")
                or relative.parts[0] == "libs"
                or relative.as_posix() == "LICENSE"):
            return relative
    elif source["id"] == "converter":
        # Keep every supplied runtime DLL and the complete converter license
        # directory. The -p webpage generator is not used by SleufBase.
        if (relative.parts[0] == "licenses" or relative.as_posix() == "README.md"
                or (len(relative.parts) == 1 and
                    (relative.name == "PotreeConverter.exe" or relative.suffix.lower() == ".dll"))):
            return PurePosixPath("converter") / relative
    return None


def extract(archive: Path, source: dict, output: Path) -> None:
    with zipfile.ZipFile(archive) as zipped:
        for member in sorted(zipped.infolist(), key=lambda item: item.filename):
            relative = extraction_path(member, source)
            if relative is None:
                continue
            target = output.joinpath(*relative.parts)
            if target.exists():
                raise ValueError(f"Duplicate runtime member: {relative}")
            target.parent.mkdir(parents=True, exist_ok=True)
            with zipped.open(member) as incoming, target.open("xb") as outgoing:
                shutil.copyfileobj(incoming, outgoing, 1024 * 1024)
            target.chmod(0o644)


def existing_is_complete(output: Path, sources: dict, require_msvc: bool = False,
                         msvc_identity: dict | None = None) -> bool:
    try:
        manifest_file = output / "runtime-manifest.json"
        if output.is_symlink() or manifest_file.is_symlink():
            return False
        manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
        if (manifest.get("format") != "sleufbase-pointcloud-runtime"
                or manifest.get("version") != 1 or manifest.get("sources") != sources["sources"]):
            return False
        records = manifest["files"]
        expected = {record["path"] for record in records}
        actual = {path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()}
        if actual != expected | {"runtime-manifest.json"}:
            return False
        if not set(REQUIRED).issubset(expected):
            return False
        if require_msvc and (not manifest.get("windows_runtime_complete")
                or not set(WINDOWS_REQUIRED).issubset(expected)):
            return False
        if msvc_identity is not None and manifest.get("msvc_runtime") != msvc_identity:
            return False
        for record in records:
            relative = PurePosixPath(record["path"])
            if relative.is_absolute() or ".." in relative.parts:
                return False
            path = output.joinpath(*relative.parts)
            if (path.is_symlink() or path.stat().st_size != record["size"]
                    or digest(path) != record["sha256"]):
                return False
        return digest(output / "Potree-NOTICES.txt") == digest(NOTICE)
    except (OSError, ValueError, KeyError, TypeError):
        return False


def msvc_files(directory: Path | None, license_file: Path | None) -> tuple[list[tuple[str, Path]], dict | None]:
    if directory is None:
        if license_file is not None:
            raise ValueError("--msvc-license requires --msvc-runtime-dir")
        return [], None
    directory = directory.resolve(strict=True)
    if not directory.is_dir() or any("debug_nonredist" in part.lower() for part in directory.parts):
        raise ValueError("Use a release x64 Visual Studio CRT redistributable directory")
    files = sorted((path.name.lower(), path) for path in directory.iterdir() if path.suffix.lower() == ".dll")
    if not set(MSVC_REQUIRED).issubset({name for name, _ in files}):
        raise ValueError("The x64 CRT directory must contain " + ", ".join(MSVC_REQUIRED))
    for name, path in files:
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Invalid redistributable DLL: {path}")
        # Inspect the PE header without executing the DLL. Accidental x86/ARM
        # libraries otherwise produce opaque loader errors on the target PC.
        with path.open("rb") as stream:
            header = stream.read(64)
            if len(header) < 64 or header[:2] != b"MZ":
                raise ValueError(f"Redistributable is not a PE file: {name}")
            stream.seek(struct.unpack_from("<I", header, 0x3C)[0])
            pe = stream.read(6)
            if pe[:4] != b"PE\0\0" or len(pe) != 6 or struct.unpack_from("<H", pe, 4)[0] != 0x8664:
                raise ValueError(f"Redistributable is not Windows x64: {name}")
    parts = directory.parts
    redist = next((i for i, part in enumerate(parts) if part.lower() == "redist"), None)
    source_directory = "/".join(parts[redist:]) if redist is not None else directory.name
    identity = {"provider": "Microsoft Visual C++ Runtime", "source_directory": source_directory,
                "files": [{"name": name, "size": path.stat().st_size, "sha256": digest(path)} for name, path in files]}
    if license_file is not None:
        if license_file.is_symlink() or not license_file.is_file() or not license_file.stat().st_size:
            raise ValueError("The supplied Microsoft runtime license file is missing or empty")
        if license_file.suffix.lower() not in (".txt", ".rtf", ".html", ".pdf"):
            raise ValueError("Microsoft runtime license must be TXT, RTF, HTML or PDF")
        identity["license"] = {"name": license_file.name, "sha256": digest(license_file)}
    return files, identity


def add_msvc_runtime(output: Path, files: list[tuple[str, Path]], identity: dict | None,
                     license_file: Path | None) -> None:
    if identity is None:
        return
    for name, source in files:
        shutil.copyfile(source, output / "converter" / name)
    licenses = output / "converter/licenses"
    notice = (
        "Microsoft Visual C++ Runtime (x64)\n"
        "Copyright Microsoft Corporation. All rights reserved.\n\n"
        "These unmodified release redistributable DLLs were copied from the build\n"
        "machine's Visual Studio redistributable installation directory:\n"
        f"  {identity['source_directory']}\n\n"
        "Use and redistribution are governed by the applicable Microsoft Software\n"
        "License Terms for that Visual Studio installation. These DLLs are not\n"
        "licensed under Potree's BSD license. Their source sizes and SHA256 hashes\n"
        "are recorded in Microsoft-VCRuntime-PROVENANCE.json and runtime-manifest.json.\n\n"
        "Microsoft license terms directory:\n"
        "  https://visualstudio.microsoft.com/license-terms/\n"
        "Microsoft Visual Studio 2022 redistributable-code list:\n"
        "  https://learn.microsoft.com/visualstudio/releases/2022/redistribution\n"
        "Microsoft redistribution guidance:\n"
        "  https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files\n"
    )
    (licenses / "Microsoft-VCRuntime-NOTICES.txt").write_text(notice, encoding="utf-8", newline="\n")
    (licenses / "Microsoft-VCRuntime-PROVENANCE.json").write_text(
        json.dumps(identity, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    if license_file is not None:
        shutil.copyfile(license_file, licenses / ("Microsoft-VCRuntime-LICENSE" + license_file.suffix.lower()))


def prepare(output: Path, cache: Path, offline: bool, force: bool,
            msvc_runtime_dir: Path | None = None, msvc_license: Path | None = None) -> Path:
    sources = json.loads(SOURCE_MANIFEST.read_text(encoding="utf-8"))
    if sources.get("format") != "sleufbase-pointcloud-runtime-sources" or sources.get("version") != 1:
        raise ValueError("Unsupported point-cloud runtime source manifest")
    dlls, msvc_identity = msvc_files(msvc_runtime_dir, msvc_license)
    require_msvc = os.name == "nt" or msvc_runtime_dir is not None
    output = output.absolute()
    if output.exists() or output.is_symlink():
        if existing_is_complete(output, sources, require_msvc, msvc_identity):
            return output
        if not force:
            raise ValueError(f"Runtime exists but is incomplete or modified: {output}. "
                             "Use --force to replace this generated directory.")
        if output.is_symlink() or not output.is_dir():
            raise ValueError("Refusing to replace a symlink or non-directory runtime")
    if os.name == "nt" and msvc_runtime_dir is None:
        raise ValueError("Windows packaging requires --msvc-runtime-dir pointing to the release x64 "
                         "Microsoft.VC*.CRT directory under Visual Studio/VC/Redist/MSVC")
    archives = [(source, obtain_archive(source, cache, offline)) for source in sources["sources"]]
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".pointcloud-runtime-", dir=output.parent) as temporary:
        staging = Path(temporary) / "runtime"
        staging.mkdir()
        for source, archive in archives:
            extract(archive, source, staging)
        add_msvc_runtime(staging, dlls, msvc_identity, msvc_license)
        shutil.copyfile(NOTICE, staging / "Potree-NOTICES.txt")
        (staging / "PROVENANCE.json").write_text(
            json.dumps(sources, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        for relative in REQUIRED:
            path = staging / relative
            if not path.is_file() or path.stat().st_size == 0:
                raise ValueError(f"Official archives are missing required runtime file: {relative}")
        records = [{"path": path.relative_to(staging).as_posix(), "size": path.stat().st_size,
                    "sha256": digest(path)}
                   for path in sorted(staging.rglob("*")) if path.is_file()]
        manifest = {"format": "sleufbase-pointcloud-runtime", "version": 1,
                    "platform": sources["platform"], "sources": sources["sources"],
                    "windows_runtime_complete": msvc_identity is not None,
                    "msvc_runtime": msvc_identity, "files": records}
        (staging / "runtime-manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
        if output.exists():
            # Never delete the old directory until the replacement is complete.
            backup = Path(temporary) / "previous-runtime"
            output.rename(backup)
            try:
                staging.rename(output)
            except OSError:
                backup.rename(output)
                raise
        else:
            staging.rename(output)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--cache-dir", type=Path, default=ROOT / ".cache/pointcloud-runtime")
    parser.add_argument("--offline", action="store_true", help="Require cached, hash-verified release archives")
    parser.add_argument("--force", action="store_true", help="Replace an incomplete generated runtime directory")
    parser.add_argument("--msvc-runtime-dir", type=Path,
                        help="Release x64 Microsoft.VC*.CRT directory from Visual Studio's VC/Redist/MSVC")
    parser.add_argument("--msvc-license", type=Path,
                        help="Optional original Microsoft license terms file to include with the runtime")
    args = parser.parse_args()
    try:
        print(prepare(args.output, args.cache_dir, args.offline, args.force,
                      args.msvc_runtime_dir, args.msvc_license))
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.exit(1, f"Point-cloud runtime could not be prepared: {error}\n")


if __name__ == "__main__":
    main()
