# Map settings and layer interaction follow-up

This extends the map navigation audit after preview 2. It does not assert complete UI parity.

## Confirmed gaps and changes

| Trigger | Before | After |
| --- | --- | --- |
| Save a different viewer background, then restart or create a new project | Viewer choice existed only in the active project | The atomic Rust settings pair also stores viewer defaults. Startup/new projects use them; saved project views take precedence. Old settings without the new field retain the previous PDOK defaults. |
| Open Settings > Maps to export offline | The renderer supported white, but the normal settings selector omitted it | White (without internet) is selectable alongside PDOK, cadastral and OSM. A4 DPI and scale are available in the same tab. |
| Change map export settings in a project that already has project-specific settings | Global draft did not describe or change the settings used by that project's export | Maps opens with the active project's map settings. Successful save updates that project and global defaults, marks the project dirty, and leaves unrelated project export preferences intact. A stale dialog cannot retarget another project. |
| Press Delete/Backspace on a selected TIFF row | Original TIFF list bindings were absent | The focused selected TIFF row supports both keys; the KLIC list also supports them. Removal uses the existing layer cleanup path and leaves source files intact. |
| Press those keys while editing/searching or after clicking the map | A global binding would risk deleting an unrelated layer | Only the selected focused list row can handle the key. Dialogs, menus, map focus, modifiers and ambiguous duplicate paths block it. Repeats cannot remove the next row. |

Original evidence: `_build_layout` in the original application's bytecode binds `<Delete>` and `<BackSpace>` to the TIFF list. The active settings navigation patch stages viewer changes until a successful save. Persisted startup viewer defaults are an added native improvement, not a claim that the original persisted them.

## Regression coverage

- Old settings deserialization and unsupported provider rejection.
- Atomic settings disk roundtrip, new-project defaults and saved-view precedence.
- Project map settings, unrelated preference preservation and actual export-worker request.
- Real pointer selection followed by Delete and Backspace; repeat safety.
- Map/dialog/focus/modifier guards; actual Backspace editing of location text.
- Physical TIFF and KLIC removal, duplicate-path rejection and byte-for-byte source preservation.
- Actual offline combo selection and visible DPI/scale/footer controls at 1200x760, 1500x930 and 1920x1080.
- Existing cancellation, save-failure, stale-generation, map export and map navigation regressions remain relevant.

## Validation boundaries

The previous native Windows map audit remains in `map-navigation-audit.md`. During this follow-up, the desktop was locked when opening the fresh isolated test profile, so additional interactive screenshots were unavailable. Automated egui tests render and operate the actual UI, but do not substitute for a native visual acceptance pass. Both LOCALAPPDATA and APPDATA were isolated for the attempted app launch.

Full GNU tests have two previously reproduced baseline font geometry failures (`lff_font` and `stroke_font`). An added old-settings test initially caught a PDOK label alias difference; the default now preserves the exact old value and the focused regression passes. The PR records the final GNU/MSVC test and build results.

Still open: comprehensive original/native acceptance of authenticated KickTheMap jobs/login/downloads, real survey-project exports and failure recovery, physical touchpad hardware, and the remaining surfaces listed in `UI_PARITY.md`. No release or blanket parity certification follows from this change.


## Preview 3 follow-up (27 October 2026)

The settings scope correction now covers the complete effective export settings, including DXF values, trench preferences, rules and preserved legacy project overrides. The previous Maps-only merge left other tabs showing global defaults even while exports used project values. Opening now stages the effective project settings; successful save applies the full candidate only to the same project generation, and saves global defaults. A scope explanation tells users to save the project and distinguishes app-wide backups.

Three additional regression tests cover actual template/DXF dialog values after save, failed/cancelled/stale drafts, native project roundtrip, and legacy overrides materializing a native settings scope without changing the preserved archive. All 186 UI tests passed locally; Clippy completed. Existing offline UI checks now include the project-scope explanation at three window sizes.

The desktop was available for this follow-up. Native Windows testing used a synthetic GeoTIFF/KLIC project and isolated LOCALAPPDATA/APPDATA. The settings window displayed the project's 12 m DXF label gap rather than the global 3.5 m default. Real UI input changed it to 7 m; Save completed and settings.json contained 7. Screenshots are retained in the task outputs. This supersedes the earlier locked-desktop limitation for these specific settings checks, not the remaining full-parity/practice acceptance limits.

The one-shot release workflow targets v0.1.0-preview.3. It only consumes a successful push build on current main, verifies both checksums and the portable payload and source revision, uploads four assets to a draft, then publishes. The prior preview tags are not overwritten. Final Windows-CI and publication results are recorded in the PR/release.
