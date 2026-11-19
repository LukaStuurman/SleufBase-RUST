# SleufBase Rust 1.0.43 — snellere KickTheMap-downloads en DXF-sjabloonexport

## KickTheMap

- **Meerdere jobs tegelijk downloaden:** bij het laden van geselecteerde jobs worden tot **drie bestanden gelijktijdig** opgehaald. De actieve, geverifieerde KickTheMap-sessie wordt uitsluitend in het geheugen gebruikt. Iedere download behoudt de controles op bestandsidentiteit, geldige serverlink, downloadlimiet en veilig wegschrijven zonder bestaande bronbestanden te overschrijven.
- **Minder onnodige serveraanvragen:** na een succesvol downloadlinkverzoek onthoudt SleufBase tijdelijk de werkende HTTP-aanvraagvorm (multipart/formulier/JSON) en probeert die eerst voor de volgende job. Als de server anders reageert, blijven de andere vormen als terugval beschikbaar.
- **Overzichtelijke voortgang:** afzonderlijke bestanden blijven met succes of foutmelding geteld worden. De normale JobFeatures-JSON- en GeoTIFF-inleesfase begint na het downloaden, zodat die processen elkaar niet annuleren.
- **Veiligheid:** het aantal parallelle downloads blijft begrensd op maximaal drie, projectwissels annuleren de oude batch, en bevestigde sessieverval stopt verdere aanvragen. Er worden geen nieuwe wachtwoorden, cookies of gesigneerde downloadlinks op schijf opgeslagen.

## DXF-sjabloonexport

- **Sneller hergebruik van afbeeldingen:** een al gevalideerde of net gemaakte PNG die ook voor de omgekeerde profielrichting nodig is, wordt gekopieerd zonder hem nogmaals volledig te decoderen. Controle op het oorspronkelijke beeld, beschikbare schijfruimte en het totale assetbudget blijft bestaan.
- **Snellere beeldverwerking:** grote (minstens 2 megapixel) kaart- en orthofoto-PNG's gebruiken nu een vaste verliesvrije PNG-rijfilter bij de bestaande snelle compressie. De kleuren, geometrie, coördinaten en beeldresolutie veranderen daardoor niet.
- **Efficiënter kopiëren:** rasterbundels gebruiken grotere blokken voor het overnemen van bestanden. De bestaande begrensde rendering en veilige publicatiestappen blijven gehandhaafd.

## Controle

De Windows-release wordt gecompileerd en de installer/portable ZIP worden door de releaseworkflow gecontroleerd. De volledige geautomatiseerde testset is volgens de eerdere voorkeur overgeslagen. De daadwerkelijke snelheidswinst kan per internetverbinding, KickTheMap-server en scan-/exportgrootte verschillen en is niet met je eigen account gemeten.
