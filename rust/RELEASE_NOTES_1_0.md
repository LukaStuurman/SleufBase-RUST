## SleufBase Rust 1.0.8 voor Windows

### Hoofdkaart en joblijst

- De hoofdkaart heeft nu afgeronde hoeken die ook zichtbaar blijven wanneer kaartbeelden en bewerkingselementen tot aan de rand lopen.
- De joblijst heeft ruimere afgeronde rijen, duidelijke jobnamen, statuslabels en een herkenbare aanduiding voor jobs die al in het project zijn geladen.
- De kop, filters en downloadacties zijn overzichtelijker vormgegeven.

### Coördinaten van het sleufmiddelpunt kopiëren

- Klik met de rechtermuisknop op een virtuele of KickTheMap-proefsleuf en kies **Coördinaten midden kopiëren**.
- De optie staat zowel op de kaart als in de proefsleuvenlijst.
- De tekst bevat alleen twee RD-coördinaten, bijvoorbeeld `155004.000, 463000.000`: zonder X-/Y-labels, met een decimale punt en een komma tussen de waarden.

### Duidelijkere DXF-sjabloonkaarten

- Meetpunten, hulppunten en meetlijnen worden herkend als meetinformatie en niet meer als blauwe kabelkruising door een virtuele sleuf getekend. Dit geldt ook voor zulke objecten in eerder opgeslagen projecten.
- De extra oranje stip met blauwe/cyaan selectiering wordt uit de kaarten van de DXF-sjabloonexport verwijderd. Kabel- en leidinglijnen gebruiken hun ingestelde kleur zonder selectieaccent.
- De zwart-witte referentiemarkering met legenda blijft beschikbaar.

### Release zonder tests

Deze release is op verzoek gebouwd zonder geautomatiseerde tests of installatieproeven uit te voeren. De exe en installer zijn gecompileerd en de downloadbestanden zijn gecontroleerd op versie, inhoud en controlesom.

### Downloaden en bijwerken

De Windows-installer en portable ZIP hebben ieder een SHA-256-controlebestand. Sluit SleufBase voor het bijwerken. Pak de portable ZIP volledig uit voordat je het programma start.

Open het project opnieuw en maak een nieuwe DXF-sjabloonexport om de aangepaste kaarten te krijgen.
