# SleufBase Rust – UI preview 5

## Eenvoudiger virtuele proefsleuven

- Twee duidelijke tabbladen voor sleufgegevens en kabel-/leidinginvoer.
- Kies een punt op de kaart of geef een afstand vanaf het begin- of eindpunt op.
- Kies het kabel-/leidingtype uit de projectlijst en vul diameter, aantal, materiaal en optioneel diepte in. Kommagetallen worden ondersteund.
- Een enkele invoer kan direct worden opgeslagen; meerdere invoeren kunnen eerst in het concept worden verzameld. Annuleren laat het project intact.
- De zeldzame schuine kruising staat onder een ingeklapte extra optie. De hoek wordt meegenomen in kaart, profiel, raster- en DXF-geometrie.
- Bestaande objectgegevens en dekbandinformatie blijven behouden bij bewerken.

## Ingebouwde KickTheMap-aanmelding

- De losse Edge-browser is vervangen door een eigen SleufBase-inlogvenster met Microsoft WebView2.
- Voltooi aanmelden en eventuele reCAPTCHA zelf in dat venster en kies daarna **Sessie overnemen** in SleufBase.
- SleufBase wacht op een gereed browservenster en toont fouten bij het starten. De tijdelijke browsersessie wordt bij afsluiten opgeruimd.
- Microsoft WebView2 Runtime is vereist. Er wordt geen apart Edge-browservenster geopend.

## Controles en beperking

Regressietests omvatten echte kaartklikken, begin-/eindafstanden, concept opslaan/annuleren, kleine vensters en DPI-schaling, typekeuze, bundelgegevens, schuine DXF-geometrie en het starten van een verborgen ingebouwde browser met een lokale testpagina. Er is geen Computer Use gebruikt.

De Windows-build, Rust-tests, Clippy, browsercontracten en controle van portable pakket en installatiebestand moeten slagen voordat deze release wordt gepubliceerd. De echte KickTheMap-accountaanmelding en reCAPTCHA zijn niet met een gebruikersaccount geverifieerd. Aanmelden blijft een handmatige stap.

Dit is een preview. De portable ZIP en installer bevatten dezelfde gecontroleerde build; SHA256-bestanden zijn toegevoegd.
