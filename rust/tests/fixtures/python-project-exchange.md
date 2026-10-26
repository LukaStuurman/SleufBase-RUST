# Original project export settings and slots

`generate_project_exchange_fixtures.py` executes hash-validated original project
settings methods and the active slot-variant patch on fake objects. It does not
start the app or read local settings/accounts. The reference records the original
18 settings, job-based layer keys, normalized PS numbers/variants, empty slots,
scale metadata and the exact active serializer/loader field names.

Native export merges representable settings and slot changes into the original
objects, retaining unknown root/layer/slot/rule fields. Ambiguous associations
and settings the unmodified Python loader cannot apply are explicit errors.
Advanced native preferences are not hidden in an extension that Python ignores.
Supported virtual-project preferences use the same original slot contract.

The Rust project tests compare these original values and verify unknown fields,
missing linked assets, overwrite protection and rollback. Full reverse conversion
of every native-only project feature and real-project acceptance remain open.
