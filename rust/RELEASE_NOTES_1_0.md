# SleufBase Rust 1.0.44 — dezelfde kaartselectie bij puntenwolken als KickTheMap

## Puntenwolken: kaart
- **Selectievak op de kaart:** sleep met de linkermuisknop een rechthoek om meerdere RD-puntenwolk-locaties te selecteren.
- **Selectie uitbreiden:** met Ctrl, Shift of Cmd kun je nieuwe puntenwolken toevoegen aan een bestaande selectie. Esc annuleert een lopend selectievak.
- **Kaartbediening gelijkgetrokken:** Passend, Zoom naar selectie, Vernieuwen, zoomen met het muiswiel en verplaatsen met middelste of rechter muisknop.
- **Meerdere scans tegelijk bekijken:** Zoom naar selectie past de kaart aan op de gezamenlijke grenzen van de geselecteerde bronnen.
- **Bestaand gedrag behouden:** dubbelklikken opent de puntenwolkeditor. Alleen puntenwolken met RD New-coördinaten worden op de Nederlandse kaart getoond; de oorspronkelijke scan- en meetdata blijven intact.

## Puntenwolken: lijst
- **Selectievakjes per rij:** selecteer meerdere scans en gebruik het selectievakje in de kop voor alle zichtbare scans.
- **Gedeelde selectie tussen kaart en lijst:** wijzigingen op de kaart zijn zichtbaar in de lijst en andersom.
- **Selectiefilters:** 'Alleen geselecteerd', 'Alle zichtbare scans selecteren' en 'Selectie wissen', met een teller voor het aantal geselecteerde scans.

## Build en controles
- Deze release wordt op verzoek **zonder geautomatiseerde tests** gebouwd. De Windows-compilatie, het samenstellen van de portable ZIP en installer en de pakket- en checksumcontrole blijven in het publicatieproces.
- De werking van de kaartselectie is niet handmatig op een Windows-computer gevalideerd voor deze release.
