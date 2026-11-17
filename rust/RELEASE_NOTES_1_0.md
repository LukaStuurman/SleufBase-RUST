## SleufBase Rust 1.0.16 voor Windows

### KickTheMap-browser en kabelgegevens

- Open de ingebouwde KickTheMap-browser vanuit **KickTheMap → Jobs → Open in KickTheMap-browser** voor de geselecteerde job.
- Vul kabels en leidingen eenvoudiger in met het nieuwe Rust-invoerpaneel voor type/naam, materiaal, aantal, diameter en dekband. Gebruik projectkeuzes, eigen waarden of gegevens uit de vorige invoer.
- **Vul gegevens in** zet de gegevens in het bijbehorende KickTheMap-formulier. Controleer de invoer en bevestig met **Opslaan** in KickTheMap.
- Bij het opnieuw openen van dezelfde job blijven het browservenster en de conceptinvoer beschikbaar. Afbreken of een fout bij het openen verwijdert een bestaand browservenster niet.
- Met **Objecten opnieuw laden** haal je de actuele jobobjecten op; de bijbehorende kaartoverlay, TIFF- en proefsleufkoppelingen worden samen bijgewerkt.

### Correcties in Jobs

- **Handmatige GeoTIFF-download vastleggen** opent eerst de geselecteerde proefsleufpagina, ook wanneer er na het aanmelden nog geen browservenster bestaat. De vastlegging start pas wanneer die pagina gereed is; daarna kun je op de GeoTIFF-download klikken.
- Je kunt de vastlegging annuleren vanuit het browserpaneel. Teruggaan, een andere aanmelding of een projectwissel kan geen late vastlegging voor de verkeerde job starten.
- De Jobs-kaart behoudt de juiste verhoudingen bij het vergroten of verkleinen van het venster.
- Sleep met de **linkermuisknop** een rechthoek om meerdere jobpunten te selecteren. Met **Ctrl** of **Shift** voeg je punten toe aan de bestaande selectie. De middelste of rechter muisknop verplaatst de kaart; het muiswiel zoomt.

### Kaart laden tijdens slepen

De hoofdkaart haalt ontbrekende kaartdelen al tijdens het slepen op. Nieuwe aanvragen worden begrensd en bruikbare lopende downloads kunnen afronden, zodat de kaart blijft reageren zonder een wachtrij per muisbeweging. De bestaande kaart blijft zichtbaar terwijl nieuwe delen binnenkomen.

### Meer ruimte en een rustigere kaartindeling

- **Kabelinformatie** staat links uitgelijnd met de andere titels in de zijbalk. De knoppen staan direct naast de titel en zijn naar links verplaatst.
- Het venster **Kabelobjecten** toont geen afzonderlijke tekst met het aantal gevonden objecten en geen sluitkruisje meer. Klik opnieuw op **Lijst** om het te sluiten.
- De kabelkaarten zijn lager en tonen alleen de laagnaam en inhoudelijke kabelinformatie. De tekst **Actief** en de lichtgrijze metadataregels zijn verwijderd; de oranje markering geeft de selectie aan. De hoogte past zich aan de tekst aan en de selectie blijft behouden bij scrollen en het wijzigen van de venstergrootte.
- De bediening uit **Weergave** staat nu in de witte strook onder **Kaart**, boven de kaart zelf. Daar kies je de kaartachtergrond en stel je de TIFF-weergave in.

### Build en controle

Deze release wordt alleen gepubliceerd na een geslaagde volledige Windows-run met Rust-tests, Clippy, browser- en formuliercontroles, de releasebuild en pakketcontrole. De installer wordt gecontroleerd op installeren, opnieuw installeren/bijwerken, bestandsintegriteit, starten en verwijderen. Versie, bronrevisie en SHA-256-controlesommen worden vóór publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start. Bij beide downloads staat een SHA-256-controlebestand.
