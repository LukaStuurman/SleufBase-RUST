# Terrain profile marker geometry

These reference values were captured from the original Python profile and map
functions with Python 3.11. The reference JSON is preserved byte for byte from
the original geometry capture; only the terrain-profile cases are used here.
No application settings, account or external service is required by these tests.
BRT/BGT responses are fixtures.

The four cases cover virtual and physical TIFF sources, fresh BRT/BGT bounds,
independent context failure, marker-expanded axes, declared band injection,
reversed band chainages, missing end-depth fallback and rotated start, middle
and end positions.

Native tests compare all marker coordinates and profile endpoints against the
captured values. They also verify that project source bytes remain unchanged
and cancellation precedes context requests. These checks protect the ordinary
manual terrain editor and do not require a model or inference runtime.
