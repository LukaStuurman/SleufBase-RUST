# Active original WMS transport and preview contract

`tools/generate_pdok_wms_fixtures.py` executes the repository's original
`pdok.py` WMS class with fake HTTP sessions/resource policy and real Pillow.
It never imports the desktop app, settings, accounts or live-render patches.
Reproduce with Python 3.11, Pillow 11.3.0 and requests in the existing dev venv;
set PYTHONUTF8=1. Output is reference.json alongside this file.

Captured contracts: exact original progress tile rectangles, progressive RGBA
pixels (including alpha over the original 243 gray canvas), final pixels,
authoritative cache hit without progress callbacks, four three-attempt retry
outcomes with 0.4/0.8 second backoff, and four Python round-to-three-decimal keys.
The native transport tests use local fake HTTP only, checking 503 and service
XML retry, PNG pixels, shared immutable cache hits, separate provider identities,
entry/byte eviction, callback cancellation and cancellation of a stalled socket.

Active source audit: PdokWmtsTileClient.fetch_map routes orthoHR layers to the
exact-size PdokWmsClient.fetch_map path. The native code already had this WMS
final path. The missing daily behavior was its cache/retry/progress envelope.
renderer.py crops visible TIFF source pixels and uses Pillow bilinear/affine
sampling. geotiff.py loads the oriented image with its tag transform. These
active paths do not select TIFF overview image directories, build pyramids or
reproject GeoTIFF pixels into a new CRS. Those features are not claimed as an
original parity requirement from this audit.

Relevant original tests: tests/test_pdok_high_quality_patch.py,
tests/test_network_cache_core_performance.py,
tests/test_renderer_viewport_stability.py and
tests/test_renderer_tiff_cache_budget.py. tests/test_resource_policy.py covers
the original hardware-dependent budgets. Native cache uses a fixed shared
96 MiB/24 entry ceiling across both fixed providers and tile/frame entries;
this is a bounded native policy, not a port of hardware-adaptive resource policy.

Native progress borrows the assembled image and provides completed/total/tile;
no full-frame copy or unbounded preview queue is imposed by core. Tiled fetches
remain sequential and use the existing four shared cancellable HTTP permits;
the original's per-client ThreadPoolExecutor timing is not claimed equivalent.
Legacy owned-image wrappers copy pixels on cache hits; SharedRaster/WmsClient
lookups expose Arc pixels without that copy. API/UI integration and full export
regressions are separate release checks.
