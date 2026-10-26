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
