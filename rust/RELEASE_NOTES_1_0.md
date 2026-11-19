# SleufBase Rust 1.0.45 — snellere DXF-sjabloonafbeeldingen

## GeoTIFF eenmaal decoderen binnen een export
- De kaart en de normale/omgekeerde profielrichting delen dezelfde originele gedecodeerde GeoTIFF-pixels. Per bewerking blijft een eigen schrijfbare pixelkopie beschikbaar, zodat rotaties, transparantie en getekende objecten elkaar niet beïnvloeden.
- De decoder gebruikt een thread-safe OnceLock: ook gelijktijdige rastertaken wachten op één fysieke TIFF-decoder. Bronbuffers worden vrijgegeven na de laatste geplande gebruiker, ook wanneer een afbeelding uit de schijfcache wordt hersteld.

## Minder onnodig roteren
- Voor bronbeelden met veel transparante pixels wordt het geldige uitvoergebied ruim berekend met behoud van het originele coördinatenstelsel en bicubische sampleposities.
- Onzichtbare buitenranden hoeven dan niet volledig gerenderd te worden. De bestaande rotatie blijft actief voor geheel gedekte beelden, kwartslagen en niet-geschikte uitsneden.
- De cropmetadata gebruikt nog altijd de afmetingen en de coördinaten van het volledige geroteerde beeld. De bestaande alfa-normalisatie en kabel-/leidingstijlen blijven dezelfde.

## Beelden tussen exports hergebruiken
- Afbeeldingen worden opnieuw gebruikt wanneer de volledige inhoud van de lokale bronbestanden en alle relevante stijl-, teken- en oriëntatiegegevens onveranderd zijn.
- De lokale beeldcache bewaart gemaakte TIFF-/virtuele detailafbeeldingen, foto's en kaartbeelden. Wijzigt de broninhoud, de instellingen of de programmaversie, dan verandert de cache-identiteit automatisch.
- PDOK-/OSM-kaarten mogen maximaal **15 minuten** uit de tijdelijke beeldcache worden hergebruikt om verouderde kaartbeelden te beperken.
- Cachebestanden worden atomair gepubliceerd, vóór hergebruik gecontroleerd en begrensd op 512 MiB en maximaal 2048 bestanden. De geproduceerde DXF verwijst altijd naar de eigen exportafbeeldingen, nooit naar de cache.
- De voortgang toont hoeveel afbeeldingen werkelijk vanuit de cache zijn hergebruikt.

## Build en beperkingen
- Op uitdrukkelijk verzoek is deze Windows-release gebouwd **zonder geautomatiseerde tests**. De normale Windows-compilatie, portable ZIP, installer en integriteitscontrole van pakketten blijven actief.
- De regressietest voor pixelidentieke sparse rotatie is als code toegevoegd maar bij deze release niet uitgevoerd.
- De snelheid van een eerste export kan vooral bij grote bronbestanden verschillen van die van een herhaalde export. Er is geen gemeten versnelling op het eigen project of de computer van de gebruiker geclaimd.
