# SleufBase Rust 1.0.46 — snellere kaarten, grote puntenwolkbatches en overzichtelijke knoppen

## Kaartachtergronden sneller laden
- **Gedeelde OpenStreetMap-kaarttegels:** de hoofdkaart, KickTheMap-jobkaart, puntenwolkenkaart en kaartuitvoer gebruiken één gezamenlijke cachemap. Dit beperkt dubbele downloads wanneer je tussen vensters wisselt.
- **PDOK-cache na opnieuw starten:** gedownloade, gecontroleerde WMS-kaartbeelden worden veilig en automatisch op schijf bewaard. Luchtfoto's maximaal zes uur, perceel-/kadasterlagen maximaal vijftien minuten. De cache wordt begrensd op 256 MiB en 2.048 bestanden en bij gebruik opgeschoond.
- **Geen onnodige dubbele PDOK-aanvragen:** hetzelfde gebied met dezelfde laag en resolutie hoeft bij gelijklopende aanvragen niet twee keer te worden opgehaald.
- **Luchtfoto en percelen tegelijk:** bij normale kaartformaten kunnen de basiskaart en de perceelgrenzen parallel worden voorbereid. Bij grotere kaarten blijft het geheugenzuinige pad actief.
- **Minder grote voorbeeldberekeningen:** OpenStreetMap tekent een al volledig opgeslagen kaartbeeld slechts één keer in plaats van nogmaals een tijdelijke afbeelding op te bouwen. Voor grote OSM- en PDOK-kaarten worden minder tussentijdse volledige previews samengesteld; de uiteindelijke resolutie verandert niet.
- **DXF-sjabloonexport:** de PDOK-tegels voor grote exportkaarten kunnen ook wanneer geen tussentijdse voortgangsafbeelding gevraagd wordt met maximaal vier begrensde werkers worden opgehaald.
- De bestaande beperkingen op parallelle netwerkverzoeken blijven gelden. Een eerste bezoek aan een gebied heeft nog steeds een netwerkverbinding nodig.

## Veel LAS/LAZ-bestanden tegelijk importeren
- In **Puntenwolk → LAS/LAZ openen** kun je maximaal **2.048 bestanden tegelijk** selecteren, in plaats van 32. De gezamenlijke wachtrij heeft een grens van 4.096 bestanden.
- Puntenwolken worden één voor één geconverteerd, zodat de importer geen honderden zware conversies tegelijk start.
- In **Puntenwolken beheren** zijn voortgang, overgeslagen reeds bewaarde scans en fouten per bestand zichtbaar. Via **Wachtrij stoppen** beëindig je een actieve conversie en wis je de resterende opdrachten zonder al bewaarde scans te verwijderen.
- De importwachtrij kan worden aangevuld tijdens verwerking. Bestaande bronidentiteiten en meetobjecten blijven behouden wanneer een lokale weergave opnieuw wordt opgebouwd.
- Het puntenwolkoverzicht blijft begrensd op 4.096 bronrecords; het aantal bronnen in één project heeft een eigen grens.

## Proefsleuf bewerken
- Onderaan staat een vaste, gescheiden en responsieve actiebalk, vergelijkbaar met de KickTheMap-joblijst.
- Kabel-/leidinginvoer is gegroepeerd. **Opslaan in project** en **Sluiten zonder opslaan** hebben een vaste plek; extra acties staan bij smalle vensters onder **Meer invoeracties**.
- Fout- en statusmeldingen blijven in de onderbalk zichtbaar.

## Build en controle
- Deze versie wordt **zonder geautomatiseerde tests** gebouwd op verzoek. Windows-compilatie, bundelen van de portable ZIP en installer, pakketcontrole en SHA-256-verificatie blijven ingeschakeld.
- Er is nog geen praktijkbenchmark die een bepaalde snelheidswinst garandeert; die hangt af van bestandsgroottes, kaartgebied, internetverbinding en een warme of lege cache.
