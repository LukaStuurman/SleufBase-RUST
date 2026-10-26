# Map navigation and UI audit — 2026-10-26

Compared Rust main `374258b4f1868077f8bca3244c728bcce9922264` (preview 2) with original SleufBase `21c24a9c47fba19ac3684f9b62e99fcd72480014`.

## Reproduced defect

On native Windows, start an empty project with the default PDOK background, wait, then wheel over the right side of the map. Preview 2 repeatedly displays only the first 512-pixel tile at the upper left while the remainder stays grey. The map widget itself has the correct full-size rectangle: this is not a nested map ScrollArea or a small pointer target.

The first progressive background preview supplies `pdok_bounds`. `canvas` included those bounds in its next fit, changing metres per pixel (a deterministic test changes 0.1 to 0.10813953488375476 without user input). `update_map_background` interprets that as another camera change, invalidates the in-flight request and cancels the remaining tiles. The next first-tile preview repeats the cycle.

The original `_on_mousewheel` captures the world point before changing metres per pixel and adjusts the camera centre to keep that point under the cursor. Rust previously scaled around the panel centre, fitted every frame, and limited zoom to 0.05–300 times the initial fit.

## Changes

- Background textures no longer influence fitting. Fit once, then retain a world camera through background arrivals, layer visibility changes and window resizing.
- Wheel and pinch zoom keep the world position under the cursor fixed. Use f64 world offsets, including when crossing the complete zoom range, and rebase the live camera to avoid accumulating large f32 pixel pans.
- Consume map wheel input once, so list scroll inertia cannot later zoom the map. Horizontal wheel pans; primary drag pans in normal navigation; middle drag also pans with drawing tools active; secondary drag does not pan.
- Bound zoom in world units (0.001–10,000 metres per logical pixel), independent of the first TIFF extent.
- Explicit file imports and “Zoom naar alles” can fit contents. Navigation changes continue to mark the durable project camera dirty.

## Regression coverage

`ui_navigation_tests.rs` drives the actual egui canvas/shell with pointer, wheel, pinch and drag events. The 13 tests cover:

1. Cursor anchoring at three pointer positions across 800×600 canvas, 1200×760, 1500×930 and 1920×1080.
2. Eight successive background replacements without camera drift.
3. Full map allocation, clipping and status separation at supported shell sizes.
4. Incremental pan; primary/middle/secondary buttons; active drawing tools; no accidental click after drag.
5. Resizing without scale/centre drift; projection round trips; viewport corner bounds.
6. Wheel consumption, absence of synthetic tail and anchored pinch.
7. World-unit zoom limits and escaping a small initial extent.
8. Native DPI 125%, 150%, 200%: pixel requests change while world coverage stays fixed.
9. Progressive background preview followed by completion: one request, no cancellation loop.
10. Sidebar/foreground location entry shields the map from wheel input; camera edits stay saveable.
11. Ordinary primary clicks remain available after zoom.
12. Hiding/restoring CAD layers and changing background extents do not refit.
13. Extreme off-centre zoom retains RD precision.

## Native Windows audit

Ran the original Python 3.11 app and the Rust app in isolated application-data directories. Used synthetic RD GeoTIFF + WATER/GAS/ELEKTRA DXF data and the public PDOK background, without production project data or account credentials. Reviewed the original, broken and corrected map; wheel/pan behaviour; TIFF/CAD/background alignment and opacity; layer visibility; cable selection and its information popup; settings Maps, DXF and Trenches tabs and bottom controls. Native screenshots are saved with the task deliverables.

Painting, picking, raster-detail requests and background requests all use the same View transform. Map clipping is rectangular and correct; rounded image-corner parity is cosmetic and outside this navigation fix. No additional speculative UI redesign is included.

## Validation and limits

The two original regressions (background camera drift and cursor anchoring) fail before the fix and pass afterwards. All 13 new navigation tests pass locally on Windows GNU. The complete local all-target run records 847 passed and 2 failed across 151 test binaries. Both failures (lff_font::original_lff_parser_inheritance_snapshots_and_all_control_geometry_match and stroke_font::original_binary_ascii_unicode_and_all_shape_instructions_match_control_geometry) reproduce identically on unmodified main 374258b using the same GNU toolchain. They are baseline font-geometry portability failures, not changes to navigation. Clippy completes successfully; existing repository warnings remain. The existing Rust Windows CI runs all native targets, Clippy, optimized MSVC build and portable/installer packaging; inspect the PR checks for its final status.

DPI is exercised through egui native-pixel inputs, not by changing the user's Windows display settings. Physical touchpad hardware, authenticated KickTheMap workflows and real survey-project acceptance were not performed. This is a reviewed preview fix, not a claim of complete product acceptance.

The old source-oracle checker is not fully reproducible against current original main: 11 recorded source hashes match (including app bytecode), app.py differs, and tools/generate_workflow_contract.py is absent from upstream. The comparison therefore uses the explicitly recorded current original checkout and its running application, rather than claiming that the old complete oracle passed.
