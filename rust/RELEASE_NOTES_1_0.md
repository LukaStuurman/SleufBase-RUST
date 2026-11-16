## SleufBase Rust 1.0.12 voor Windows

### Leesbare formulieren en gelijke proefsleufkaarten

- Contactvelden in Instellingen en de Tekeningstempel in DXF-sjabloonexport gebruiken gemeten, vaste labelkolommen. Postcode, Telefoon, Website, Referentie Tekeningen, ProjectNr.BGM en TEK.NR. worden niet meer in losse woordfragmenten afgebroken.
- Invoervakken staan uitgelijnd naast de labels. Bij weinig ruimte staan complete labels boven de invoervakken; de contactvelden worden op brede schermen niet onnodig lang.
- Alle proefsleufkaarten in de sjabloonexport hebben dezelfde breedte, ook bij verschillende bronnen zoals Virtueel en KickTheMap. Lange bestandsnamen blijven binnen de kaart; de volledige naam is via de aanwijzer beschikbaar.

### Alle volgordeopties bij elkaar rechts

- Omhoog en Omlaag blijven bovenaan het rechterpaneel bereikbaar.
- Sorteren, selectie, nummering, varianten, profielschaal en lege/extra vakken staan daaronder. In de projectexport staan daar ook de acties voor alle proefsleuven en dwarsprofielen.
- De bediening scrolt onafhankelijk van de proefsleufkaarten. Na verplaatsen blijft de gekozen regel zichtbaar; exporteren en annuleren blijven onderaan bereikbaar.
- De standaardvenstermaat biedt meer ruimte voor deze verdeling en blijft begrensd door het scherm.

### Donkere achtergrond bij voortgang

Het voortgangsvenster houdt zijn lichte vormgeving, terwijl de kaart en andere inhoud erachter duidelijk donkerder worden. De bestaande blokkering van kaartbediening tijdens de taak blijft behouden.

### Kabelinformatie in het vaste paneel

- Kabelinformatie benut de hele kaartbreedte, met de scrollbar rechts langs het paneel.
- XDATA InfraCAD en Laag staan boven de overige objectgegevens.
- Een kabelklik vult het informatiepaneel zonder apart pop-upvenster. De geselecteerde kabel krijgt een vaste gloed met behoud van de oorspronkelijke kabelkleur in het midden.
- Bij overlappende objecten kun je binnen Kabelinformatie de sectie Andere objecten op deze plek openen om een andere kabel te kiezen. Een klik naast de kabels heft de selectie op.

### Grote KLIC-tekeningen op de werkkaart

De kaartweergave en kabelselectie gebruiken een ruimtelijke selectie van de DXF-geometrie. De renderer verwerkt zichtbare delen en hergebruikt de kaartgeometrie zolang de camera en laagzichtbaarheid gelijk blijven. Bronentiteiten, kabelgegevens en exportgeometrie blijven behouden. Er is geen prestatiebenchmark uitgevoerd.

### Alle jobpunten aan of uit

Naast Verwijder selectie in Overlays staat Jobpunten. Hiermee toon of verberg je alle jobpunten tegelijk. Bij gedeeltelijke zichtbaarheid toont de schakelaar een gemengde toestand; aanklikken maakt alle jobpunten zichtbaar. DXF-overlays houden hun afzonderlijke zichtbaarheid. De keuze wordt in het project bewaard.

### Build en release

De geautomatiseerde tests, Clippy en installatieproeven worden op verzoek overgeslagen. De code wordt op compileerbaarheid gecontroleerd; de Windows-app, portable ZIP en installer worden gebouwd. De releasebestanden worden gecontroleerd op versie, inhoud en SHA-256-controlesom voor publicatie.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
