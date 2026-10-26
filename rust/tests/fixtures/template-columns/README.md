# Linked MTEXT columns in copied sheets

The isolated original exporter writes R2007, R2010, R2013 and R2018 drawings
after extensions to 9 and 17 slots. Cases include paper/model space, rotated
static columns, background/frame, XDATA and parent/child dictionaries. No app,
accounts, settings or live services are started.

`generate_template_column_fixtures.py` retains the complete original serialized
snapshots. The original ezdxf copy loses extension dictionaries on copied linked
columns; `verify_native_template_columns.py` compares every remaining field with
that output and independently requires the complete dictionary payload of every
native parent and child to match its source. Native copies retain those original
metadata losses as a deliberate fix, with independent owners and child handles.
The verifier also checks layout membership and error/fix-free CAD audits.

Missing/cyclic links and cancellation must preserve the entire source drawing.
The actual driver is `export_template_columns`. This contract concerns copying
template objects. Full column/background/embedded rendering on the map remains
a separate acceptance requirement.
