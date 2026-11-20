#!/usr/bin/env python3
"""Download a small fixed-RD georeferenced overview from the official PDOK WMS.

This source image is CC BY 4.0: (c) Samenwerkingsverband Beeldmateriaal,
https://creativecommons.org/licenses/by/4.0/ . Attribution is displayed in
the map UI. The app's six-month updater uses the same BBOX and layer.
"""
from __future__ import annotations
import argparse
import os
from pathlib import Path
import struct
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

URL = "https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0"
# EPSG:28992: min_x, min_y, max_x, max_y, metres. Y points upward.
BOUNDS = (-100000, 200000, 400000, 700000)
SIZE = 1536
MAX_DOWNLOAD = 9 * 1024 * 1024
LAYER = "Actueel_orthoHR"


def jpeg_size(blob: bytes) -> tuple[int, int]:
    """Return dimensions from SOF, rejecting WMS exception bodies and trash."""
    if not blob.startswith(b"\xff\xd8") or not blob.endswith(b"\xff\xd9"):
        raise ValueError("Not a complete JPEG response")
    pos = 2
    sof = {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}
    while pos + 4 < len(blob):
        if blob[pos] != 0xFF:
            raise ValueError("Invalid JPEG segment")
        while pos < len(blob) and blob[pos] == 0xFF:
            pos += 1
        if pos >= len(blob):
            break
        marker = blob[pos]
        pos += 1
        if marker in (0x01, 0xD8, 0xD9) or 0xD0 <= marker <= 0xD7:
            continue
        if pos + 2 > len(blob):
            break
        length = struct.unpack_from(">H", blob, pos)[0]
        if length < 2 or pos + length > len(blob):
            raise ValueError("Invalid JPEG segment length")
        if marker in sof:
            if length < 7:
                raise ValueError("Invalid JPEG frame")
            height, width = struct.unpack_from(">HH", blob, pos + 3)
            return width, height
        pos += length
    raise ValueError("JPEG dimensions not found")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    params = {
        "SERVICE": "WMS", "VERSION": "1.3.0", "REQUEST": "GetMap",
        "LAYERS": LAYER, "STYLES": "", "CRS": "EPSG:28992",
        "BBOX": ",".join(map(str, BOUNDS)),
        "WIDTH": str(SIZE), "HEIGHT": str(SIZE),
        "FORMAT": "image/jpeg", "TRANSPARENT": "FALSE",
        "BGCOLOR": "0xE5E9E8",
    }
    url = URL + "?" + urlencode(params)
    last = None
    for attempt in range(3):
        try:
            request = Request(url, headers={
                "User-Agent": "SleufBase-Rust/1.0 (offline low-resolution overview)",
                "Accept": "image/jpeg"
            })
            with urlopen(request, timeout=90) as reply:
                if "image/jpeg" not in reply.headers.get("Content-Type", "").lower():
                    raise ValueError("PDOK response is not image/jpeg")
                blob = reply.read(MAX_DOWNLOAD + 1)
            if not 12_000 <= len(blob) <= MAX_DOWNLOAD:
                raise ValueError(f"Unexpected overview size: {len(blob)} bytes")
            if jpeg_size(blob) != (SIZE, SIZE):
                raise ValueError(f"Incorrect JPEG dimensions: {jpeg_size(blob)}")
            args.output.parent.mkdir(parents=True, exist_ok=True)
            temp = args.output.with_suffix(".jpg.tmp")
            temp.write_bytes(blob)
            os.replace(temp, args.output)
            print(f"PDOK overview stored: {args.output} ({len(blob)} bytes); source is CC BY 4.0")
            return
        except (OSError, HTTPError, URLError, ValueError) as error:
            last = error
            print(f"Attempt {attempt+1} failed: {error}", flush=True)
            if attempt < 2:
                time.sleep(4 * (attempt + 1))
    raise SystemExit(f"Could not obtain a valid PDOK overview: {last}")

if __name__ == "__main__":
    main()
