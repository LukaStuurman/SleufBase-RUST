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

### Eigen DXF-sjabloon beheren

- **Instellingen > DXF-export > DXF-sjabloon** bevat drie acties: **Ingebouwd sjabloon opslaan…**, **Aangepast sjabloon laden…** en **Origineel herstellen**.
- Sla de originele DXF op, pas die aan in een CAD-programma en laad de bewerkte versie terug. SleufBase controleert het bestand en bewaart een eigen kopie voor de sjabloonexport.
- **Opslaan** past de sjabloonkeuze toe; **Annuleren** behoudt de bestaande keuze. Herstellen schakelt terug naar de ongewijzigde ingebouwde versie en laat de aangepaste bestanden bewaard.
- Het opslaan en verwerken van sjabloonbestanden gebeurt op de achtergrond, met een zichtbare status in Instellingen.

### Sneller doorgaan bij bewerkpunten

- **Proefsleufnaam** staat links van **Maaiveld invullen** en schakelt de namen op de werkkaart direct aan of uit. De knop bewaart dezelfde voorkeur als Instellingen.
- **Acties > Laad alle maaiveldgegevens** bereidt de maaiveldpunten van de geladen proefsleuven vooraf voor, met een compacte voortgangsbalk en annulering. Geldige voorbereidingen worden hergebruikt; ingevoerde tekst, kleuren en gemeten hoogten blijven behouden.
- Bij het kiezen van een andere GeoTIFF blijft **Beginpunt kiezen** beschikbaar en worden de nieuwe kandidaten automatisch voorbereid. **Maaiveld invullen** blijft actief bij het wisselen tussen proefsleuven, zowel via de kaart als de lijst.
- Een klik op een bestaande bewerkmarker houdt voorrang. Een open maaiveldtekst wordt beschermd tegen een onbedoelde selectiewissel.

### Build en release

Op verzoek worden geen runtime- of geautomatiseerde tests, Clippy of installatieproeven uitgevoerd. De compileercontrole en Windows-build verzorgen de uitvoerbare app, portable ZIP en installer. Versie, pakketinhoud en SHA-256-controlesommen worden voor publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
