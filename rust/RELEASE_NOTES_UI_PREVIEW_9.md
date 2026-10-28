# SleufBase Rust 0.1.0 — UI preview 9

Deze preview bouwt voort op UI preview 8 en bevat:

- **DXF-sjabloonexport:** proefsleuven staan bij een nieuwe export automatisch in numerieke volgorde (PS1, PS2, PS10). Extra sorteeropties: PS-nummer oplopend/aflopend en bestandsnaam A–Z/Z–A. Handmatig verschuiven en eerder opgeslagen vakindelingen blijven intact.
- **TIFF-dekking:** een nieuw project begint met de transparantieschuif op 100% zichtbaarheid. Een bestaande in een project opgeslagen instelling blijft behouden.
- **KickTheMap-objectpunten:** in Jobs is de knop **Objectpunten downloaden + laden** beschikbaar. Alle geladen JobFeatures-objectpunten en -lijnen verschijnen op de kaart en als afzonderlijke aan-/uitzetbare joblagen in het KLIC-overlaysvenster. De zichtbaarheid wordt per oorspronkelijke bron opgeslagen zonder dubbele DXF-bestanden.
- **Validatie:** nieuwe regressietests voor sorteren, projectweergave, de jobs-lagenlijst en het behoud van opgeslagen instellingen.

## Bestanden

- **SleufBase-Rust-preview-Setup.exe** — Windows-installer.
- **SleufBase-Rust-portable-preview.zip** — portable Windows-versie.
- De bijbehorende SHA256-bestanden laten controle van beide downloads toe.

De publicatieworkflow publiceert uitsluitend een volledig geteste Windows-build van de actuele main-commit, verifieert de ZIP/installer en vervangt nooit een bestaande release.
