# SleufBase Rust UI preview 2

Dit is een **aparte Windows-preview** van de native Rust-app. De originele SleufBase (Python) wordt niet vervangen.

## Verbeteringen

- Jobs-browser met zeven kolommen, kolomfilters, kaartweergave en **Open job** via KickTheMap.
- Fysieke TIFF-laag: rechtermuisknopmenu met **Maak virtuele kopie**, rotatie en opmerkingen.
- Virtuele TIFF: hoek invoeren, draaien met een interactieve kaartgreep, verplaatsen, hernoemen, kopiëren en opmerkingen.
- Punt op kettingmaat met afzonderlijke stappen en aparte dekbandbeheervensters.
- **marXact kabel-/leidingoriëntatie** met correcties van -90° tot +90° en opnieuw automatisch bepalen.
- **KickTheMap DXF-export** met directe selectie en import van live JobFeatures. Niet-downloadbare jobs kunnen niet aangevinkt worden.
- **Gevonden lagen**-popup voor overlappende CAD-objecten.
- Verbeterde muisbediening en annuleren van kaartacties.

## Windows-bestanden

- `SleufBase-Rust-portable-preview.zip` — uitpakken en `sleufbase-rust.exe` starten, zonder installer.
- `SleufBase-Rust-preview-Setup.exe` — Windows installer.
- De twee `.sha256`-bestanden dienen voor integriteitscontrole.

## Bekende beperkingen

Dit is **geen definitieve, volledig UI-identieke Python-vervanger**. Niet alle vensters zijn pixelgelijk aan Tk en een live end-to-endtest met een echte KickTheMap-account is nog niet uitgevoerd. Een virtuele kopie van een fysieke TIFF zet de georeferentie om in een bewerkbare sleufgeometrie; de bronrasterpixels worden niet gedupliceerd. Volledige acceptatievoorwaarden staan in `rust/UI_PARITY.md`.

De Windows-installatiebestanden zijn als preview gebouwd en niet digitaal ondertekend.
