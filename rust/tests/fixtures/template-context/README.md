# Active template context and WFS grids

The original startup installs `template_wireframe_context_patch` version 3.
Its local margin is exactly ten times the original 35–75 metre margin. It merges
intersecting context areas and retains distant areas individually, including
selections beyond twelve areas. It does not clip back to the overview extent.

`generate_template_context_fixtures.py` invokes that active original method and
the original WFS grid planner on synthetic bounds. The references include eighty
distant areas, overlapping wide contexts, the 700 metre direct-query boundary,
350 metre tiles with 8 metre overlap, and the native 4096-query budget boundary.
Template queries can cross the interactive 3.5 km viewport limit. Interactive
queries retain their existing span and 100-tile limits.

The separate template-server fixture exercises provider composition and actual
written DXFs. No real-service acceptance or unlimited export budget is claimed.
