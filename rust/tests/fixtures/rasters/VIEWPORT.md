# Camera-dependent preview evidence

The original renderer.py `_paint_axis_aligned_tiff` crops visible source pixels
before bilinear display resizing. `_native_tiff_rgba_cache` retains complete
source RGBA. It does not select unrelated TIFF image directories as overviews.
Original geotiff.py `load_geotiff` uses Pillow's already oriented image dimensions
with the unchanged tag transform. Pillow orientation1..8 fixture inspection
confirms 3x2 for orientations1..4 and 2x3 for orientations5..8 before image.load.

The native window decoder uses the same oriented full-image texture coordinates,
rounds camera bounds outward to source pixels and skips unsampled strips/tiles.
It preserves the existing first-image-directory policy and full export API.
Viewport scheduling uses four affine camera corners, a 25% gutter, resolution
buckets, a 128 MiB aggregate crop budget, one decoder, cooperative cancellation
and generation rejection. The UI retains a separately budgeted base preview.

Native tests compare crop RGBA byte-for-byte with full-resolution oriented
source RGBA for all eight orientations, associated and unassociated alpha,
palette and planar fixtures. Camera tests exercise affine bounds, small-pan
reuse, zoom refinement and rejection of stale project results.

The 4097x4097 large-striped fixture has 257 strips of 16 rows. Its test requests
a 256px overview and a source-resolution 0.40..0.42 camera detail. It verifies
fewer decoded strips, nonzero skipped chunks, bounded peak chunk allocation and
exact recovery of the full-resolution source pixel. This is work/memory evidence,
not a machine-independent latency claim or a new image-directory overview policy.
