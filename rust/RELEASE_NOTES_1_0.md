## SleufBase Rust 1.0.13 voor Windows

### Geen proefsleufnamen bij Weergave: Geen

- **Weergave proefsleuf: Geen** onderdrukt de proefsleufnamen in het DXF-overzicht, ook wanneer **GeoTIFF-afbeeldingen meenemen in DXF-export** aan staat.
- Oude proefsleufnamen uit een eigen sjabloon of voorbereide exportinvoer worden op de gereserveerde modelspace-laag leeggemaakt. Afbeeldingen, entiteitshandles en tekenvolgordeverwijzingen blijven behouden.
- Voorbereide sjabloonexporten gebruiken de actuele overzichtsinstellingen. Ook een al geopend kadastraal exportvenster neemt gewijzigde algemene instellingen mee, met behoud van expliciete keuzes in dat venster.
- Hartlijn en Vlak behouden hun proefsleufnamen. Vaste koppen bij detail- en profielvakken en andere kaartteksten blijven buiten deze overzichtsinstelling.

### Kabels wisselen zonder scrollen

Naast **Kabelinformatie** staan twee compacte knoppen:

- De **wisselknop** kiest de volgende zichtbare kabel op dezelfde plek en gaat na de laatste weer naar de eerste.
- **Lijst** opent een keuzelijst met laagnaam en XDATA InfraCAD-informatie, zodat je gericht een kabel kunt kiezen.

De knoppen blijven boven het informatievak bereikbaar. Bij een nieuwe keuze begint de detailinformatie weer bovenaan en volgt de kabelgloed de selectie. De oude keuzelijst onderaan het informatievak is verwijderd.

### Lichtere KLIC-overzichten bij uitzoomen

- Drukke overzichten worden als een transparant, herbruikbaar kaartbeeld op schermresolutie bewaard. Hierdoor wordt een tekening die te groot is voor de vectorcache niet meer bij ieder schermframe volledig opnieuw opgebouwd.
- Tijdens zoomen en verschuiven beweegt het bestaande overzicht direct mee. Kort na de beweging wordt het beeld voor de nieuwe uitsnede opnieuw opgebouwd; laagzichtbaarheid en kleuren worden meegenomen.
- Lange paden krijgen een schaalafhankelijke vereenvoudiging. Details kleiner dan een beeldpunt worden compact weergegeven en onleesbaar kleine teksten komen terug bij inzoomen. De voorbereiding gebeurt tijdens het laden van de KLIC.
- De volledige DXF-geometrie blijft beschikbaar voor kabelselectie, informatie en export. In een rustige of ingezoomde uitsnede blijven de gewone fijne vectorlijnen in gebruik.

### Build en release

Op verzoek worden geen runtime- of geautomatiseerde tests, Clippy of installatieproeven uitgevoerd. De compileercontrole en Windows-build verzorgen de uitvoerbare app, portable ZIP en installer. Versie, pakketinhoud en SHA-256-controlesommen worden voor publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
