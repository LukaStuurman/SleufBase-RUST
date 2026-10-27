# Map cache, trench focus and dialogs

## Reported defects and corrections

The exact viewport BBOX previously changed every background WMS tile after any pan. A 24-entry mixed tile/frame cache also thrashed on larger views, and partial frames replaced a complete visible map. Requests now align to a stable 512-pixel world grid with quarter-octave resolution levels and a buffered margin. Active and completed coverage is reused for nearby motion. Only missing edge tiles are fetched; the last complete visible image stays until the replacement completes. Desktop tiles share the existing byte quota with a 512-entry cap, without caching a duplicate assembled desktop frame. Extreme/high-DPI views retain the bounded 10,000-pixel/50-megapixel fallback.

TIFF rows had no double-click action. A double-click now selects and fits the clicked physical raster or virtual measured boundary using the actual canvas dimensions. Single-click selection retains the camera. Rotated physical bounds and project-change cancellation are covered.

Settings used default sizing and expandable content with nearly white, rounded checkbox icons. The window now has fixed viewport-relative geometry, a fixed footer and scrollable content. All categories use bordered groups; Maps uses two columns when space permits. Checkbox icons are square, outlined and have a contrasting check mark. The logo selector shows readable names.

The login menu previously redirected to the browser/Jobs workspace instead of showing an account form. It now opens a directly reachable account login window with email/password entry, masked text, blank-input validation, cancel/close cleanup and the existing API/browser login handlers. No credentials are persisted in projects.

## Evidence

- Actual local HTTP server: a 36-tile frame followed by a one-column pan makes 42 total requests, not 72. Thirty overlap tiles retain cached pixels at their correct world positions.
- Scheduler tests: nearby pan retains in-flight work and completed coverage; detail zoom/provider changes invalidate appropriately; request budgets remain bounded.
- Actual shell/render event tests: small pan retains one fetch; double-click fits a virtual or 37-degree rotated physical trench at 1200x760 and 1500x930, while single-click and source data remain unchanged.
- The exact reported native hover-growth trigger was not reproduced in an old-default standalone headless case. No native automation was resumed. Production-theme/DPI testing did expose a first-layout position shift caused by automatic Area constraints; fixed viewport-relative placement now avoids that too. The corrected sizing is explicit and its regression uses the production theme, three viewport sizes and 100/125/200% rendering scales.
- UI hover tests: moving the pointer over every settings category and the words editor leaves the window rectangle unchanged at compact, minimum and default sizes. Checkbox outlines and both toggle states are exercised.
- Login menu, masked rendering, keyboard field navigation, validation with no network, password cleanup and minimum-size geometry are exercised through egui events.
- Generated CPU raster images of the actual UI meshes are reviewed for all settings tabs and the login form. These are not native desktop screenshots.

Computer Use was explicitly stopped and subsequently prohibited by the user. No further desktop automation was used for these corrections. Real account authentication, physical touchpads and practice-project acceptance remain unverified. Final full-suite/Windows CI/release results are recorded with the PR and release.
