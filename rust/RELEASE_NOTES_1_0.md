## SleufBase Rust 1.0.15 voor Windows

### Kabels selecteren met een lijn

- Rechts van **Lijst (aantal)** bij **Kabelinformatie** staat een nieuwe compacte knop met een getekend lijnicoon.
- Klik op de knop, klik op het beginpunt in de kaart en klik daarna op het eindpunt. De lijn volgt de muis tijdens het tekenen.
- Alle zichtbare kabels en lijnobjecten die het getekende lijnstuk raken of kruisen komen in de kandidatenlijst rechts naast de kaart. Verborgen tekeningen en lagen worden overgeslagen.
- **Lijst (aantal)** toont het nieuwe aantal. De eerste kandidaat is direct geselecteerd; de wisselknop, de lijst, de kabelinformatie en de gloed op de kaart blijven dezelfde selectie gebruiken.
- De selectie gebruikt de oorspronkelijke voorbereide lijngeometrie, ook bij uitgezoomde weergave en kabels in blokken. Eindpunten en overlappende lijnstukken tellen mee; teksten, losse punten en gevulde vlakken niet.
- Grote selecties worden in kleine stappen verwerkt met zichtbare voortgang, zodat de kaart bedienbaar blijft. Resultaten worden niet afgekapt.
- **Escape**, rechtsklikken of opnieuw op de lijnknop klikken annuleert de lopende selectie. De vorige kandidatenlijst blijft dan behouden.
- De ingebouwde handleiding legt de lijnselectie en het wisselen tussen de resultaten uit.

### Afgeronde knoppen en passende indeling

- **Proefsleufnaam** behoudt zijn afgeronde hoeken wanneer de knop oranje en ingeschakeld is.
- De nieuwe lijnknop sluit aan bij de bestaande wisselknop. Beide hebben een compact, scherp getekend icoon en uitleg bij het aanwijzen.
- De knoppen naast **Kabelinformatie** houden hun ruimte bij een smalle zijbalk; de titel breekt niet op in losse letters.

### Build en release

Op verzoek worden geen runtime- of geautomatiseerde tests, Clippy of installatieproeven uitgevoerd. De compileercontrole en Windows-build verzorgen de uitvoerbare app, portable ZIP en installer. Versie, pakketinhoud en SHA-256-controlesommen worden voor publicatie gecontroleerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
