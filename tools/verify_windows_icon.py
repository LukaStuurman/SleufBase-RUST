"""Verify the primary icon embedded in a Windows executable against its ICO asset."""
from __future__ import annotations

import argparse
from collections import Counter
from pathlib import Path
import struct


def _slice(data: bytes, offset: int, size: int) -> bytes:
    if offset < 0 or size < 0 or offset + size > len(data):
        raise ValueError("Truncated Windows icon or executable resource")
    return data[offset:offset + size]


def _resources(data: bytes) -> dict:
    if data[:2] != b"MZ":
        raise ValueError("Executable is not a Windows PE image")
    pe = struct.unpack("<I", _slice(data, 60, 4))[0]
    if _slice(data, pe, 4) != b"PE\0\0":
        raise ValueError("Executable has no PE signature")
    sections = struct.unpack("<H", _slice(data, pe + 6, 2))[0]
    optional_size = struct.unpack("<H", _slice(data, pe + 20, 2))[0]
    optional = pe + 24
    magic = struct.unpack("<H", _slice(data, optional, 2))[0]
    if magic not in (0x10B, 0x20B):
        raise ValueError("Executable has an unknown PE optional header")
    directories = optional + (112 if magic == 0x20B else 96)
    resource_rva, resource_size = struct.unpack("<II", _slice(data, directories + 16, 8))
    if not resource_rva or not resource_size:
        raise ValueError("Executable contains no embedded resources")

    def at_rva(rva: int, size: int) -> bytes:
        for index in range(sections):
            header = optional + optional_size + index * 40
            _, address, raw_size, raw_offset = struct.unpack("<IIII", _slice(data, header + 8, 16))
            if address <= rva and rva + size <= address + raw_size:
                return _slice(data, raw_offset + rva - address, size)
        raise ValueError("Icon resource is outside the PE sections")

    tree = at_rva(resource_rva, resource_size)
    result = {}

    def walk(offset: int, path: tuple) -> None:
        if len(path) >= 3:
            raise ValueError("Unexpected icon resource directory depth")
        named, numbered = struct.unpack("<HH", _slice(tree, offset + 12, 4))
        for index in range(named + numbered):
            key, target = struct.unpack("<II", _slice(tree, offset + 16 + index * 8, 8))
            if key & 0x80000000:
                name_offset = key & 0x7FFFFFFF
                length = struct.unpack("<H", _slice(tree, name_offset, 2))[0]
                key = _slice(tree, name_offset + 2, length * 2).decode("utf-16-le")
            if not path and key not in (3, 14):  # RT_ICON / RT_GROUP_ICON
                continue
            next_path = path + (key,)
            if target & 0x80000000:
                walk(target & 0x7FFFFFFF, next_path)
            else:
                if len(next_path) != 3:
                    raise ValueError("Incomplete icon resource path")
                rva, size = struct.unpack("<II", _slice(tree, target, 8))
                result[next_path] = at_rva(rva, size)

    walk(0, ())
    return result


def _frame(entry: bytes, payload: bytes) -> tuple:
    width, height, colors, reserved, planes, bits, size = struct.unpack("<BBBBHHI", entry[:12])
    if size != len(payload):
        raise ValueError("Icon directory size differs from its image resource")
    if payload.startswith(b"\x89PNG\r\n\x1a\n"):
        if _slice(payload, 8, 8) != b"\0\0\0\rIHDR":
            raise ValueError("PNG icon has no valid IHDR")
        png_width, png_height, depth, color_type = struct.unpack(
            ">IIBB", _slice(payload, 16, 10)
        )
        if (width or 256, height or 256) != (png_width, png_height):
            raise ValueError("Icon directory dimensions differ from its PNG image")
        # RC derives PNG planes/bpp from the image: 1 plane, 32 bits. Pillow ICO
        # files use planes=0 (unspecified). Normalize only that harmless 0/1
        # distinction for actual 8-bit RGBA PNGs; every other field and every
        # image byte must still match. See LLVM's compatible RC implementation:
        # llvm/tools/llvm-rc/ResourceFileWriter.cpp, visitIconOrCursorResource.
        if depth == 8 and color_type == 6 and bits == 32 and planes in (0, 1):
            planes = 1
    return ((width, height, colors, reserved, planes, bits, size), payload)


def verify(executable: Path, icon: Path) -> int:
    ico = icon.read_bytes()
    reserved, kind, count = struct.unpack("<HHH", _slice(ico, 0, 6))
    if reserved or kind != 1 or not count:
        raise ValueError("Expected a nonempty ICO asset")
    expected = []
    for index in range(count):
        entry = _slice(ico, 6 + index * 16, 16)
        size, offset = struct.unpack_from("<II", entry, 8)
        expected.append(_frame(entry, _slice(ico, offset, size)))

    resources = _resources(executable.read_bytes())
    # Windows uses the first icon group as the application's default icon.
    group = next(((path, data) for path, data in resources.items() if path[0] == 14), None)
    if group is None:
        raise ValueError("Executable has no primary icon group")
    path, data = group
    if _slice(data, 0, 6) != ico[:6]:
        raise ValueError("Primary executable icon has different image sizes or count")
    embedded = []
    for index in range(count):
        entry = _slice(data, 6 + index * 14, 14)
        resource_id = struct.unpack_from("<H", entry, 12)[0]
        payload = resources.get((3, resource_id, path[2]))
        if payload is None:
            raise ValueError("Primary icon group references a missing icon image")
        embedded.append(_frame(entry, payload))
    if Counter(embedded) != Counter(expected):
        if Counter(frame[0] for frame in embedded) != Counter(frame[0] for frame in expected):
            raise ValueError(
                "Primary executable icon metadata differs from the TechBase ICO asset: "
                f"embedded={[frame[0] for frame in embedded]}, expected={[frame[0] for frame in expected]}"
            )
        raise ValueError("Primary executable icon image bytes differ from the TechBase ICO asset")
    return count


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("icon", type=Path)
    args = parser.parse_args()
    count = verify(args.executable, args.icon)
    print(f"Verified TechBase executable icon: {count} embedded image sizes")


if __name__ == "__main__":
    main()
