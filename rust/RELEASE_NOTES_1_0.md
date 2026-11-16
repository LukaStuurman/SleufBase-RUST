## SleufBase Rust 1.0.14 voor Windows

### Kandidatenlijst naast de kaart

- **Lijst (3)** naast **Kabelinformatie** toont het actuele aantal gevonden kabels bij de laatste kaartklik.
- De knop opent een paneel rechts naast de kaart met alle kandidaten, hun laagnaam en beschikbare **XDATA InfraCAD**-informatie.
- Klik op een regel om die kabel te selecteren. Het paneel blijft open om andere kandidaten te vergelijken; een nieuwe kaartklik werkt de lijst en de telling bij.

### Eenduidig wisselen en selecteren

- De wisselknop kiest daadwerkelijk de volgende kabel uit dezelfde kandidatenlijst en gaat na de laatste terug naar de eerste.
- De actieve lijstregel, **Kabelinformatie** en de gloed op de kaart volgen steeds hetzelfde geselecteerde object. De detailinformatie begint na een nieuwe keuze weer bovenaan.
- De ingebouwde handleiding beschrijft de kandidatenlijst, de actuele telling en het wisselen tussen kabels.

### PS-naam naast de TIFF bij Weergave: Geen

- **Weergave proefsleuf: Geen** laat de hartlijn en het sleufvlak weg. Als **GeoTIFF-afbeeldingen meenemen in DXF-export** aan staat, blijven de TIFF én het PS-nummer of de proefsleufnaam naast het beeld zichtbaar.
- De correctie geldt voor het kadastrale DXF-overzicht en het overzicht in de sjabloonexport. Bij **Geen** met GeoTIFF-afbeeldingen uit worden ook de TIFFs en hun proefsleufnamen weggelaten.
- De toelichting in Instellingen en de ingebouwde handleiding beschrijft deze combinatie.

### Luchtfoto of OpenStreetMap met percelen

- **PDOK Luchtfoto + percelen** en **OpenStreetMap + percelen** zijn beschikbaar als achtergrond voor de werkkaart en kaartuitvoer.
- Beide combineren de gekozen achtergrond met transparante perceelgrenzen, perceelnummers en bijpijlingen uit de kadastrale kaart van Kadaster/PDOK. De achtergrond blijft zichtbaar; de perceeldetails volgen het zoomniveau van de kaartdienst.
- De keuze wordt bewaard in de instellingen en het Rust-project. Bestaande kaarttegels, cache en achtergrondtaken worden hergebruikt.

### Consistente vormgeving

- Alle scrollbalken gebruiken dezelfde duidelijk zichtbare stijl, breedte en kleuren, in **Proefsleuven**, **Overlays**, **Kabelinformatie**, de kandidatenlijst, instellingen, exportvensters, de handleiding en menu’s.
- De overbodige tekst **GeoTIFF en KLIC** boven de kaart is verwijderd; de titel **Kaart** blijft staan.

### Build en release

Op verzoek worden geen runtime- of geautomatiseerde tests, Clippy of installatieproeven uitgevoerd. De compileercontrole en Windows-build verzorgen de uitvoerbare app, portable ZIP en installer. Versie, pakketinhoud en SHA-256-controlesommen worden voor publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
