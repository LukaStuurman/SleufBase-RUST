# Original pending AHN and incremental native results

`tools/generate_ahn_pending_fixtures.py` isolates the unchanged original app
code objects for _queue_pending_virtual_trench_surface_lookups,
_virtual_trench_point_uses_auto_surface and _virtual_trench_point_surface_status.
No original app/settings/accounts are initialized. Ten queue cases cover
normalized auto/pending versus resolved/unavailable/manual/start/absent values.
Reproduce with Python 3.11 and PYTHONUTF8=1; output override is
SLEUFBASE_AHN_PENDING_FIXTURES.

The original per-point background workers enqueue each result independently;
_poll_background_queue applies them without waiting for all points. Native
Worker::new and with_streaming_fetcher now publish one point at a time through
a capacity-one result channel. busy stays true until the terminal message;
terminal success is an empty Vec. poll returns at most one message, preserving
an earlier valid/unavailable point before a later panic or failure. One worker,
one queued task and one latest pending task bound scheduling. Cancellation and
project/source/point/metadata identity gates reject late results. Shared Arc
source snapshots and the persistent provider/cache avoid full source clones.
with_processor intentionally retains batch-injection compatibility for callers.

The original AHN provider's default is one attempt for each of the 2/8/24 meter
search windows, with 12 second request timeout and a 512 entry two-decimal LRU.
No extra automatic retry loop is invented. Explicit error/unavailable results
are published immediately and do not stop later point requests.

Deliberate protection: the original _sync_virtual_trench_object_surfaces_to_start
blanket-overwrites object heights. Native keeps measured/manual elevations and
only propagates start ground level to explicitly attached objects. Regression
tests exercise manual, measured, movement, generation and cancellation guards.
Original tests/test_reliability.py contains the provider's bounded parallel-cache
test. No dedicated original pending-app unit module was found; the isolated
code-object oracle and native streaming/stale/worker tests cover that contract.
