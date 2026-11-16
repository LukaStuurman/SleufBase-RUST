## SleufBase Rust 1.0.10 voor Windows

### Eén verwijderknop per lijst

- In **Proefsleuven** en **Overlays** staat nu één knop **Verwijder selectie**, met het aantal geselecteerde items ernaast.
- De aparte knop **Wis selectie** is verwijderd. Met Ctrl+klik kun je items nog steeds aan de selectie toevoegen of eruit halen; Shift en Ctrl+Shift blijven beschikbaar voor reeksen.
- De verwijderknop is alleen actief als er iets is geselecteerd. Geselecteerde lagen worden samen uit het project verwijderd; de bronbestanden blijven behouden.
- De grijze achtergrondvlakjes achter de namen in beide lijsten zijn verwijderd. Geselecteerde regels blijven herkenbaar gemarkeerd en de zichtbaarheidsschakelaars blijven beschikbaar.

### Leesbaar infovenster in dezelfde stijl als Instellingen

- De onderwerpen staan overzichtelijk onder elkaar in een eigen navigatiekolom. Lange titels worden netjes afgekort; de volledige titel verschijnt bij aanwijzen en boven de uitleg.
- De uitleg van het gekozen onderwerp staat rechts, met duidelijke koppen, alinea’s en een eigen scrollgebied.
- Het infovenster sluit qua vormgeving aan op Instellingen. Op smalle vensters blijft een compacte onderwerpkeuze beschikbaar.
- Zoeken, F1, de vermelding **Gemaakt door Luka Stuurman** en het actuele versienummer blijven beschikbaar.

### Gelijke titelbalken en vensterranden

- De pop-upvensters in de app gebruiken dezelfde lichte titelbalk, vensterrand, afgeronde hoeken en schaduw.
- Het actieve venster krijgt geen afwijkende grijze bovenbalk meer. Ook Jobs en de voortgangsvensters volgen nu dezelfde vensterstijl als Instellingen en Info.

### Build en release

Zoals gevraagd worden voor deze release geen geautomatiseerde tests, Clippy of installatieproeven uitgevoerd. De Windows-app en installer worden gebouwd; versie, pakketinhoud en SHA-256-controlesommen worden gecontroleerd voor publicatie.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
