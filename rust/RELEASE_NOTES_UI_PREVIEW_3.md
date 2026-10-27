# SleufBase Rust UI preview 3

Deze Windows-preview verbetert de kaartbediening en de instellingen. De originele SleufBase-installatie blijft apart beschikbaar.

## Verbeteringen

- De kaart vult de volledige viewport: voortgang van achtergrondtegels verandert de camera niet meer en annuleert de overige tegels niet.
- Zoomen blijft bij de cursor; pannen, laagzichtbaarheid en vensterformaat behouden de kaartpositie. RD-coördinaten blijven nauwkeurig.
- Kaartachtergrond en zichtbaarheid zijn te bewaren als standaard voor herstart en nieuwe projecten; opgeslagen projecten behouden hun eigen weergave.
- Instellingen → Kaarten biedt offline witte uitvoer, A4-resolutie (DPI) en schaal.
- Het instellingenvenster toont nu de werkelijk actieve projectinstellingen voor kaarten, DXF en proefsleuven. Opslaan werkt het actieve project en de standaard bij; geannuleerde, mislukte of verouderde dialogen veranderen geen ander project.
- Delete/Backspace werkt op de geselecteerde TIFF- of KLIC-rij met focus. Tekstvelden, kaartfocus, dialogen, herhaalde aanslagen en dubbele bestandspaden zijn afgeschermd. Bronbestanden worden niet verwijderd.

## Windows-downloads

- **SleufBase-Rust-portable-preview.zip**: uitpakken en `sleufbase-rust.exe` starten.
- **SleufBase-Rust-preview-Setup.exe**: aparte Windows-installatie.
- De twee `.sha256`-bestanden controleren de downloads.

Sla het project op om gewijzigde projectinstellingen te behouden. Back-upinstellingen gelden voor de hele app.

## Controle en beperkingen

Deze release wordt uitsluitend gepubliceerd uit een geslaagde Windows-MSVC-run op de huidige main. Die run test alle native targets, voert Clippy uit, bouwt de app en verifieert het portable pakket en de installer. De publicatiestap controleert ook de pakketinhoud, revisie en SHA-256-bestanden.

Dit blijft een preview: volledige visuele gelijkwaardigheid met de Python-app, echte ingelogde KickTheMap-workflows en acceptatie met praktijkprojecten zijn nog niet vastgesteld. De installatiebestanden zijn niet digitaal ondertekend. Tijdens de voorafgaande lokale GNU-controle falen twee bestaande fontgeometrie-tests ook op ongewijzigde main; Windows-MSVC is de release-toolchain. Zie `rust/UI_PARITY.md` en de auditverslagen in `rust/verification/`.
