"""Regression checks for RC's PNG icon metadata normalization."""
from copy import deepcopy
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

from verify_windows_icon import verify


class WindowsIconTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.executable = Path(self.directory.name) / "app.exe"
        self.executable.write_bytes(b"PE parsing is covered by the built-executable CI check")
        self.icon = Path(__file__).resolve().parents[1] / "assets/techbase_icon.ico"
        ico = self.icon.read_bytes()
        self.count = struct.unpack_from("<H", ico, 4)[0]
        group = bytearray(ico[:6])
        self.resources = {}
        # Model RC output from the real asset. Its PNG entries say planes=0;
        # RC writes planes=1 and retains every PNG image byte unchanged.
        for index in range(self.count):
            entry = bytearray(ico[6 + index * 16:6 + (index + 1) * 16])
            self.assertEqual(struct.unpack_from("<H", entry, 4)[0], 0)
            size, offset = struct.unpack_from("<II", entry, 8)
            resource_id = 100 + index
            self.resources[(3, resource_id, 0x409)] = ico[offset:offset + size]
            struct.pack_into("<H", entry, 4, 1)
            group.extend(entry[:12] + struct.pack("<H", resource_id))
        self.resources[(14, 1, 0x409)] = bytes(group)

    def verify_resources(self, resources):
        with patch("verify_windows_icon._resources", return_value=resources):
            return verify(self.executable, self.icon)

    def test_accepts_rc_normalized_png_planes(self):
        self.assertEqual(self.verify_resources(self.resources), self.count)

    def test_accepts_preserved_unspecified_png_planes(self):
        resources = deepcopy(self.resources)
        group = bytearray(resources[(14, 1, 0x409)])
        for index in range(self.count):
            struct.pack_into("<H", group, 6 + index * 14 + 4, 0)
        resources[(14, 1, 0x409)] = bytes(group)
        self.assertEqual(self.verify_resources(resources), self.count)

    def test_rejects_changed_png_bytes(self):
        resources = deepcopy(self.resources)
        image = bytearray(resources[(3, 100, 0x409)])
        image[-1] ^= 1
        resources[(3, 100, 0x409)] = bytes(image)
        with self.assertRaisesRegex(ValueError, "image bytes differ"):
            self.verify_resources(resources)

    def test_does_not_normalize_other_png_formats(self):
        resources = deepcopy(self.resources)
        # Give both source and executable the same RGB PNG header. Their 0/1
        # plane fields must not be equated by the RGBA-only exception.
        ico = bytearray(self.icon.read_bytes())
        image_offset = struct.unpack_from("<I", ico, 18)[0]
        ico[image_offset + 25] = 2
        image = bytearray(resources[(3, 100, 0x409)])
        image[25] = 2
        resources[(3, 100, 0x409)] = bytes(image)
        self.icon = Path(self.directory.name) / "rgb.ico"
        self.icon.write_bytes(ico)
        with self.assertRaisesRegex(ValueError, "metadata differs"):
            self.verify_resources(resources)

    def test_rejects_other_group_metadata_changes(self):
        # Width, height, color count, reserved byte, planes, bit depth and size
        # must all remain checked despite accepting the RC plane normalization.
        for offset, fmt, value in [(0, "B", 17), (1, "B", 17), (2, "B", 1),
                                   (3, "B", 1), (4, "H", 2), (6, "H", 24), (8, "I", 1)]:
            with self.subTest(offset=offset):
                resources = deepcopy(self.resources)
                group = bytearray(resources[(14, 1, 0x409)])
                struct.pack_into("<" + fmt, group, 6 + offset, value)
                resources[(14, 1, 0x409)] = bytes(group)
                with self.assertRaises(ValueError):
                    self.verify_resources(resources)

    def test_rejects_missing_png_resource(self):
        resources = deepcopy(self.resources)
        del resources[(3, 100, 0x409)]
        with self.assertRaisesRegex(ValueError, "missing icon image"):
            self.verify_resources(resources)


if __name__ == "__main__":
    unittest.main()
