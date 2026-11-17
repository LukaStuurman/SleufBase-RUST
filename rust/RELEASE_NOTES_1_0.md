## SleufBase Rust 1.0.16 voor Windows

### KickTheMap-browser en kabelgegevens

- Open de ingebouwde KickTheMap-browser vanuit **KickTheMap → Jobs → Open in KickTheMap-browser** voor de geselecteerde job.
- Vul kabels en leidingen eenvoudiger in met het nieuwe Rust-invoerpaneel voor type/naam, materiaal, aantal, diameter en dekband. Gebruik projectkeuzes, eigen waarden of gegevens uit de vorige invoer.
- **Vul gegevens in** zet de gegevens in het bijbehorende KickTheMap-formulier. Controleer de invoer en bevestig met **Opslaan** in KickTheMap.
- Bij het opnieuw openen van dezelfde job blijven het browservenster en de conceptinvoer beschikbaar. Afbreken of een fout bij het openen verwijdert een bestaand browservenster niet.
- Met **Objecten opnieuw laden** haal je de actuele jobobjecten op; de bijbehorende kaartoverlay, TIFF- en proefsleufkoppelingen worden samen bijgewerkt.

### Meer ruimte en een rustigere kaartindeling

- **Kabelinformatie** staat links uitgelijnd met de andere titels in de zijbalk. De knoppen staan direct naast de titel en zijn naar links verplaatst.
- Het venster **Kabelobjecten** toont geen afzonderlijke tekst met het aantal gevonden objecten en geen sluitkruisje meer. Klik opnieuw op **Lijst** om het te sluiten.
- De kabelkaarten zijn lager en passen zich aan de hoeveelheid tekst aan, zodat meer objecten tegelijk zichtbaar zijn. De selectie blijft behouden bij scrollen en het wijzigen van de venstergrootte.
- De bediening uit **Weergave** staat nu in de witte strook onder **Kaart**, boven de kaart zelf. Daar kies je de kaartachtergrond en stel je de TIFF-weergave in.

### Build en controle

Deze release wordt alleen gepubliceerd na een geslaagde volledige Windows-run met Rust-tests, Clippy, browser- en formuliercontroles, de releasebuild en pakketcontrole. De installer wordt gecontroleerd op installeren, opnieuw installeren/bijwerken, bestandsintegriteit, starten en verwijderen. Versie, bronrevisie en SHA-256-controlesommen worden vóór publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start. Bij beide downloads staat een SHA-256-controlebestand.
