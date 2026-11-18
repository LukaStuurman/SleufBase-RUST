# SleufBase Rust 1.0.40 — geen zwarte sleufgrens op de puntenwolkkaart

## Verbeteringen

- In de kleine **kaart met legenda** van de DXF-sjabloonexport krijgt een ingeladen LAS/LAZ-puntenwolk geen zwarte rechthoek of sleufomtrek meer over zijn orthofoto. De zwart getekende extra polyline werd abusievelijk ook toegevoegd bij een echte puntenwolk-GeoTIFF.
- **Referentiepunt blijft behouden**, evenals de getekende kabel-/leidinglijnen, de gegeorefereerde orthofoto en het dwarsprofiel.
- Voor oudere exportinvoer met een virtuele puntenwolkbron wordt de extra zwarte omtrek ook in de kaart-renderer overgeslagen.
- Bij **gewone virtuele proefsleuven en MarXact-sleuven** blijft hun bestaande grensweergave ongewijzigd.

## Controle

De Windows-release wordt met de bestaande build- en pakketverificatie gepubliceerd. De geautomatiseerde tests worden volgens verzoek overgeslagen. Een interactieve controle op de Windows-computer met het oorspronkelijke LAS/LAZ-scanbestand is niet uitgevoerd.
