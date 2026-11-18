# SleufBase Rust 1.0.38 — orthofoto van puntenwolken in zeer hoge resolutie

## Nieuw en verbeterd

- **Geen kunstmatige virtuele proefsleufafbeelding voor LAS/LAZ-sleuven.** In de DXF-sjabloonexport gebruikt een puntenwolkproefsleuf uitsluitend de eigen gegeorefereerde orthofoto. De extra rastervermelding voor dezelfde scan verdwijnt uit de standaardlijst. Gewone handmatig getekende virtuele proefsleuven houden hun bestaande rasterweergave.
- **Eigen bestandsnaam:** de orthofoto wordt opgeslagen als `<naam-van-de-puntenwolk>.tif` in de map van de betreffende scan. Verboden Windows-tekens worden veilig vervangen. Bij hernoemen wordt de nieuwe foto met de nieuwe naam opnieuw aangemaakt. De naam is herkenbaar in de GeoTIFF-lijst en DXF-export.
- **Veel hogere resolutie:** het bovenaanzicht wordt maximaal **8.192 pixels** langs de langste zijde, met een geheugenveilig plafond van **36 megapixels**. Vierkante scans worden proportioneel geresampled om onder die limiet te blijven. De rastercellen worden op basis van RGB, intensiteit of hoogte van de echte scanpunten gevormd; getekende Potree-punten, lijnmarkeringen en labels zitten niet in deze orthofoto.
- **Oriëntatie op het begin- en eindpunt:** de volledige RD New (EPSG:28992) affine matrix blijft in de GeoTIFF opgeslagen. Een gewijzigd begin-/eindpunt of een nieuwe titel zorgt dat de orthofoto opnieuw wordt opgebouwd.
- **Bewuste verwerking:** bij het aanvankelijk importeren van meerdere scans wordt nog geen zware 8K-orthofoto gemaakt. Zodra begin en eind zijn getekend en je de sleuf inlaadt, wordt het raster in de achtergrond voorbereid en daarna als echte kaartlaag toegevoegd. De projectgegevens en originele LAS/LAZ worden niet aangepast of verwijderd.
- **Automatische bronkoppeling:** bestaande DXF-sjablooninstellingen met het oude `orthoview.tif` worden opnieuw aan de juiste scanorthofoto gekoppeld. Als die afbeelding nog niet klaar is, krijg je een duidelijke melding in plaats van de kunstmatige virtuele afbeelding.

De eerdere correcties voor oorspronkelijke 3D-polylijnen, directe inlaadfunctie, bulkimport, Potree-kleuren en muisknoppen blijven behouden.

## Controle

De Windows-releasebuild en controles van installer en portable ZIP vinden plaats via GitHub Actions. De volledige geautomatiseerde tests blijven volgens afspraak overgeslagen. De hoge-resolutieweergave is nog niet interactief met jouw Windows-scanbestand gecontroleerd.
