#!/usr/bin/env python3
"""Compare real template CLI exports; no application Python modules are imported.

Only owned bundle paths and AcDbDynamicBlockGUID/1000 identities are normalized.
Timings are observations from these runs, not a performance benchmark.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parent.parent
GUID = re.compile(rb"\{[0-9A-Fa-f]{8}-(?:[0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}\}")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def absolute(value):
    if not value:
        return value
    path = Path(value)
    return str((REPO / path).resolve() if not path.is_absolute() else path.resolve())


def resolve_inputs(request):
    request["template"] = absolute(request["template"])
    for dataset in request.get("datasets", []):
        if dataset.get("source_path"):
            dataset["source_path"] = absolute(dataset["source_path"])
    settings = request.setdefault("settings", {})
    settings["export_date"] = [2026, 10, 26]
    for key, value in settings.get("logos", {}).items():
        if value:
            settings["logos"][key] = absolute(value)
    settings["font_directories"] = [absolute(p) for p in settings.get("font_directories", [])]
    for slot in filter(None, request["slots"]):
        for key in ("tiff_png", "map_png"):
            if slot.get(key):
                slot[key] = absolute(slot[key])
        if slot.get("tiff_source"):
            slot["tiff_source"]["path"] = absolute(slot["tiff_source"]["path"])
        for photo in slot.get("photos", []):
            photo["path"] = absolute(photo["path"])
        source = slot.get("map_source")
        if source:
            background = source.get("background", {})
            if background.get("kind") == "png":
                background["path"] = absolute(background["path"])
            if source.get("tiff"):
                source["tiff"]["path"] = absolute(source["tiff"]["path"])
            for key in ("regular", "bold"):
                if (source.get("fonts") or {}).get(key):
                    source["fonts"][key] = absolute(source["fonts"][key])
        trenches = [(slot.get("virtual_source") or {}).get("trench"),
                    (slot.get("profile") or {}).get("virtual_trench")]
        for trench in filter(None, trenches):
            if trench.get("source_path"):
                trench["source_path"] = absolute(trench["source_path"])
    return request


def scenarios():
    rows = json.loads((REPO / "rust/tests/fixtures/template-export/reference.json").read_text(encoding="utf-8"))
    fixtures = {row["name"]: row["request"] for row in rows if row["name"] in ("mixed", "native-tiff")}
    photo = str((REPO / "rust/tests/fixtures/template-export/tiff.png").resolve())
    for name in ("mixed", "physical-paired", "virtual-paired", "virtual-reference"):
        request = copy.deepcopy(fixtures["mixed" if name == "mixed" else "native-tiff"])
        if name != "mixed":
            request.setdefault("settings", {})["paired"] = True
        for slot in filter(None, request["slots"]):
            slot["photos"] = [{"path": photo, "turns": 1, "area": "tiff"}]
            if name != "mixed":
                slot.pop("map_png", None)
                slot["map_source"] = {
                    "settings": {"dpi": 30, "scale": 225.0},
                    "background": {"kind": "solid", "color": [245, 245, 245, 245]},
                }
            if name.startswith("virtual"):
                slot.pop("tiff_png", None)
                slot.pop("tiff_source", None)
                bounds = slot["bounds"]
                slot["virtual_source"] = {"quality": 0.2, "reverse": False, "trench": {
                    "label": slot["label"], "width_meters": 0.6,
                    "start": {"x": bounds["min_x"] + 2, "y": bounds["min_y"] + 3, "z": 1.0},
                    "end": {"x": bounds["max_x"] - 2, "y": bounds["max_y"] - 3, "z": 1.0},
                }}
                if slot.get("profile"):
                    slot["profile"]["show_reference"] = name == "virtual-reference"
        yield name, resolve_inputs(request)


def normalize_dxf(path, bundle):
    data = path.read_bytes()
    prefix = str(bundle.resolve(strict=True)).encode("utf-8")
    if prefix not in data:
        raise RuntimeError("DXF does not reference the canonical owned bundle path")
    data = data.replace(prefix, b"OWNED_ASSETS")
    lines = data.splitlines(keepends=True)
    identities = {}
    # A DXF is code/value pairs. Preserve every byte outside these GUID values,
    # including code padding, line endings, and identity reuse/order.
    for index in range(0, len(lines) - 3, 2):
        if (lines[index].strip() == b"1001"
                and lines[index + 1].rstrip(b"\r\n") == b"AcDbDynamicBlockGUID"
                and lines[index + 2].strip() == b"1000"):
            raw = lines[index + 3].rstrip(b"\r\n")
            if not GUID.fullmatch(raw):
                raise RuntimeError("Malformed AcDbDynamicBlockGUID/1000 value")
            identity = identities.setdefault(raw, len(identities))
            ending = lines[index + 3][len(raw):]
            lines[index + 3] = f"DYNAMIC_GUID_{identity}".encode("ascii") + ending
    return b"".join(lines), len(identities)


def first_difference(a, b):
    offset = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
    line = a[:offset].count(b"\n") + 1
    return (f"byte {offset}, line {line}; lengths {len(a)} / {len(b)}; "
            f"context {a[max(0, offset-40):offset+100]!r} / {b[max(0, offset-40):offset+100]!r}")


def run_export(executable, root, request):
    root.mkdir(parents=True)
    manifest = root / "request.json"
    manifest.write_text(json.dumps(request, ensure_ascii=False, indent=2), encoding="utf-8")
    output = root / "output.dxf"
    # This fresh, deliberately absent path isolates preferences even if the CLI
    # settings loading changes later. No existing settings are inspected.
    settings = root / "fresh-settings.json"
    command = [str(executable), "--settings", str(settings), "--template-dxf", str(output), str(manifest)]
    started = time.perf_counter()
    try:
        process = subprocess.run(command, cwd=root, capture_output=True, timeout=120, check=False)
    except subprocess.TimeoutExpired as error:
        raise RuntimeError(f"{executable.name}: template CLI exceeded 120 seconds") from error
    elapsed = time.perf_counter() - started
    if process.returncode:
        tail = (process.stderr or process.stdout).decode("utf-8", errors="replace").strip()[-800:]
        raise RuntimeError(f"{executable.name}: CLI exit {process.returncode}: {tail}")
    bundles = sorted((root / "output_assets").glob("export-*"))
    if len(bundles) != 1 or not bundles[0].is_dir():
        raise RuntimeError(f"{executable.name}: expected exactly one owned export bundle, got {len(bundles)}")
    bundle = bundles[0]
    files = sorted(bundle.iterdir(), key=lambda p: p.name)
    if not files or any(not p.is_file() or p.suffix.lower() != ".png" for p in files):
        raise RuntimeError(f"{executable.name}: owned bundle has missing or unexpected non-PNG assets")
    hashes = {p.name: sha256(p) for p in files}
    normalized, guid_count = normalize_dxf(output, bundle)
    return normalized, {"wall_seconds": elapsed, "png_count": len(hashes), "png_sha256": hashes,
                        "normalized_dxf_sha256": hashlib.sha256(normalized).hexdigest(),
                        "dynamic_guid_count": guid_count}


def write_proof(path, proof):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n", dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        json.dump(proof, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--current", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    baseline, current = args.baseline.resolve(strict=True), args.current.resolve(strict=True)
    proof = {"status": "running", "normalization": "owned bundle path and ordered AcDbDynamicBlockGUID/1000 identities only",
             "timing_note": "Single-run wall timings; no speed claim.",
             "worker_overrides": {key: os.environ.get(key) for key in
                                  ("SLEUFBASE_RESOURCE_MODE", "SLEUFBASE_MAX_WORKERS", "SLEUFBASE_MAP_WORKERS", "SLEUFBASE_RASTER_WORKERS")},
             "executables": {"baseline": {"name": baseline.name, "sha256": sha256(baseline)},
                             "current": {"name": current.name, "sha256": sha256(current)}}, "scenarios": []}
    try:
        with tempfile.TemporaryDirectory(prefix="sleufbase-asset-equivalence-") as folder:
            for name, request in scenarios():
                a, old = run_export(baseline, Path(folder) / name / "baseline", request)
                b, new = run_export(current, Path(folder) / name / "current", request)
                result = {"name": name, "baseline": old, "current": new, "equal": False}
                proof["scenarios"].append(result)
                if old["png_sha256"] != new["png_sha256"]:
                    names = sorted(set(old["png_sha256"]) | set(new["png_sha256"]))
                    first = next(n for n in names if old["png_sha256"].get(n) != new["png_sha256"].get(n))
                    raise RuntimeError(f"{name}: PNG filename/hash differs: {first}")
                if a != b:
                    raise RuntimeError(f"{name}: DXF differs at {first_difference(a, b)}")
                result["equal"] = True
                print(f"{name}: equal ({old['png_count']} PNGs)", flush=True)
        proof["status"] = "passed"
    except (OSError, RuntimeError, ValueError) as error:
        proof["status"] = "failed"
        proof["error"] = str(error)
        print(f"Equivalence failed: {error}", file=sys.stderr)
    write_proof(args.output.resolve(), proof)
    return 0 if proof["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
