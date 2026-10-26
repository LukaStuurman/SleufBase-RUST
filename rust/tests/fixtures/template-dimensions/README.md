# Original layout-copy dimensions

Five original dimension variants cover linear, radius, diameter, angular and
ordinate geometry, nonzero insertion offsets, named resources, XDATA and owned
extension dictionaries on dimensions and primitives. The active original page
extension helper creates three states for 9, 17 and 33 requested slots.

Expected snapshots come from serialized and reread original DXFs. Explicit and
omitted DXF defaults are canonicalized using the entity's declared default;
all other attributes and all virtual primitive geometry remain strictly compared.
This accounts for identity extrusion/scale values materialized by transforms.

Native copies retain block coordinates and insertion placement, give every
dimension an independent anonymous geometry block and copy owned metadata.
The independent verifier checks 50 dimensions across three actual native DXFs,
exact XDATA/XRECORD values, independent owners, unchanged named resources and
absence of CAD audit errors or repairs. Missing/cyclic geometry and cancellation must
leave the source document unchanged. This evidence does not cover every possible
third-party dimension subclass or replace AutoCAD acceptance.
