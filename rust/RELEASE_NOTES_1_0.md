# SleufBase Rust 1.0.35 — lijnen in DXF en bovenaanzicht van puntenwolken

## DXF en kaartweergaven

- Getekende Potree- en KickTheMap-polylijnen blijven volledig bewaard als 3D-lijn, naast de afgeleide kruispunten. Bij DXF-sjabloonexport wordt de oorspronkelijke lijn in het dwarsprofiel geprojecteerd en tevens getekend op de virtuele detail-TIFF en kaart. Voor bestaande projecten met een puntenwolkbronlaag herstelt de exporter eerder opgeslagen oorspronkelijke lijnen.
- Wanneer een echte GeoTIFF als achtergrond wordt gebruikt, worden de lijnvertices met de juiste RD-transform over de TIFF getekend voordat die naar de paginaoriëntatie wordt gedraaid en uitgesneden. De kaart ernaast toont dezelfde lijn. Originele bron-TIFF-bestanden blijven ongewijzigd.
- Sleufas/begin/eind, objectkleuren en normale/reverse-varianten volgen de bestaande proefsleufcoördinaten. Geen annotatiepunten worden in de nieuwe scanondergrond geprojecteerd.

## Puntenwolken toevoegen, later bewerken en rechtstreeks inladen

- Kies meerdere .las/.laz-bestanden tegelijk; de wachtrij bereidt ze na elkaar voor en bewaart ze in het puntenwolkoverzicht. Er wordt niet na ieder bestand automatisch de Potree-editor geopend. Klik later op **Bewerken** om punten en kabel-/leidingslijnen toe te voegen.
- Met complete begin- en eindpunten maakt **Inladen** de proefsleuf direct op de SleufBase-kaart, zonder een extra bezoek aan de virtuele proefsleufeditor. Een bestaande sleuf uit één scan wordt standaard bijgewerkt. **Inladen met opties** in Meer blijft beschikbaar voor breedte, kopieën, objectkeuze en oudere aslijnen. Bij ontbrekende eindpunten volgt een gerichte melding.
- Bij het verwerken van een scan met bevestigde RD-coördinaten wordt uit de puntwolk zelf een nieuw gegeorefereerd **GeoTIFF-bovenaanzicht** opgebouwd. RGB heeft voorrang; zonder kleur wordt intensiteit of hoogte als grijswaarde gebruikt. Lege kleine rastergaten worden geïnterpoleerd. De oorspronkelijke LAS/LAZ, 3D-punten en getekende annotaties blijven ongewijzigd.
- Het gegenereerde bovenaanzicht bevat **geen toegevoegde punten, lijnen of labels** van de editor. Na het inladen van een proefsleuf wordt het als RD GeoTIFF aan de kaartlagen toegevoegd; de DXF-sjabloonexport kan het als fysiek gegeorefereerde TIFF naast het profiel gebruiken.
- Als je RD pas later bevestigt, wordt het GeoTIFF bij het inladen asynchroon opgebouwd. De proefsleuf verschijnt direct; de afbeelding wordt na voltooiing automatisch toegevoegd. Van scans zonder bevestigde RD-coördinaten wordt geen foutief gegeorefereerde kaartafbeelding gemaakt.

## Compatibiliteit

- Nieuw opgeslagen virtuele proefsleuven kunnen een extra lijst met originele gemeten lijnen bevatten. Oudere projecten blijven leesbaar; het bronbestand wordt niet aangetast.
- Voor een project dat op een andere computer wordt gebruikt, moeten ook de originele LAS/LAZ en eventuele cache-afbeeldingen worden meegenomen of opnieuw opgebouwd.

## Controle

De Windows-releasebuild en pakketcontrole worden door GitHub uitgevoerd. De volledige geautomatiseerde testset blijft volgens de eerdere afspraak uitgeschakeld. De GUI is niet interactief met een aangeleverd Windows-scanbestand getest.
