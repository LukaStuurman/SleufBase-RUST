# SleufBase Rust v1.0.47 — Google Maps-achtige PDOK-luchtfoto en scherpere perceelkaart

## PDOK-luchtfoto laadt via WMTS-tegels
- De interactieve PDOK-luchtfoto gebruikt de officiële WMTS-tegelpiramide van PDOK, met aparte 256-pixeltegels op meerdere zoomniveaus in RD New (EPSG:28992).
- Eerst kan een grovere luchtfoto van het gebied verschijnen. Daarna worden scherpere tegels op het juiste zoomniveau opgehaald, bij voorkeur vanuit het midden van de kaart.
- Bij verschuiven of zoomen blijven eerder getoonde beelden tijdelijk zichtbaar en kunnen reeds gedownloade tegels opnieuw worden gebruikt.
- Eerder gedownloade luchtfototegels worden tijdelijk op schijf bewaard. De cache heeft een grens van 512 MiB en maximaal 4.096 bestanden, en oude bestanden worden zo nodig opgeruimd.
- De kaart blijft de bestaande WMS-downloadmethode ondersteunen als WMTS niet beschikbaar is. DXF-sjabloonexport blijft WMS gebruiken voor de exacte geografische uitsnede.

## Scherpere Kadaster-percelen
- Bij ver inzoomen worden de echte kadastrale grenzen, gebouwcontouren en labels via PDOK WFS als vectorgeometrie opgehaald en getekend.
- De automatische vectorlaag is los van handmatig ingeladen Kadaster-bestanden en wordt tijdelijk opgebouwd voor het huidige kaartgebied.
- Op grote gebieden en wanneer de vectorbron niet beschikbaar is, gebruikt de kaart een raster-/tegelachtergrond van Kadaster (WMTS/WMS). Daardoor blijft ook bij uitzoomen de kaart bruikbaar.
- Deze kaart is een visuele ondergrond en geen rechtsgeldige grensmeting.

## Virtuele proefsleuf bewerken
- De knoppen onderaan zijn compacter en blijven in een vaste actiebalk dicht tegen de onderkant van het bewerkvenster staan.
- Op brede schermen kunnen invoer- en projectacties naast elkaar staan; op kleinere vensters schakelt de indeling automatisch over.
- Tijdens het aanpassen van de diameter verschijnt de gekozen maat meteen als schaalbare cirkel met Ø-label in de doorsnede, ook wanneer er nog geen diepte is ingevoerd.
- Zonder gemeten of ingevoerde diepte staat duidelijk aangegeven dat het om een illustratief voorbeeld gaat; het programma slaat daardoor niet automatisch een verzonnen diepte op.

## Bouwen en valideren
- Op verzoek zijn de geautomatiseerde tests en Clippy-controles voor deze release overgeslagen.
- De Windows-releasecompilatie, het maken en controleren van de portable ZIP, de installer en de checksums blijven onderdeel van de publicatie.
- De snelheid en werking van de nieuwe WMTS-lader moeten in de praktijk op verschillende internetverbindingen en kaartgebieden worden gecontroleerd.
