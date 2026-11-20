# SleufBase Rust v1.0.61 — PDOK, offline zoom en beheerde puntenwolken

## Kaart en bronvermelding
- De standaard werkkaart bij nieuwe installaties en projecten is de **online PDOK-luchtfoto**. Een offline luchtfoto downloaden verandert die kaartvoorkeur niet langer. Een bewust gekozen achtergrond in Instellingen blijft bewaard.
- De wettelijk vereiste bronvermelding **Luchtfoto © Samenwerkingsverband Beeldmateriaal · CC BY 4.0** blijft zichtbaar, maar zonder storende witte rechthoek: compacte witte tekst met subtiele schaduw op de kaart.
- Hetzelfde geldt voor de aanvullende Kadaster/PDOK-bronvermelding.

## Offline luchtfoto: scherpe bron altijd behouden, ook bij uitzoomen
- Gedownloade originele WMTS JPEG's op het **hoogste gekozen zoomniveau** blijven volledig intact op 256 × 256 pixels en worden nooit in lagere resolutie vervangen.
- Bij uitzoomen bouwt SleufBase zelfstandig de lagere *weergaveniveaus* op uit de hoogste opgeslagen luchtfoto, zonder extra downloads. Daardoor is bij ver uitzoomen de hogere kwaliteit de basis, in plaats van de landelijke lage-resolutiefoto waar scherpe offline tegels beschikbaar zijn.
- Reeds aanwezige foto's uit v1.0.60 krijgen hun weergaveniveaus bij het starten op een achtergrondworker terug, zonder opnieuw te downloaden.
- Bij het verwijderen van een offline kaartgebied worden bijbehorende scherpe JPEG's alleen verwijderd als ze niet door een andere download worden gedeeld. Lokaal samengestelde zoomoverzichten worden opnieuw opgebouwd uit de overgebleven scherpe foto's.
- De bestaande limiet van 4.096 hoogste-resolutietegels per downloadgebied, de JPEG-compressiekeuze en de parallelle downloader blijven behouden.

## Puntenwolken en AppData
- Bij een nieuwe handmatige LAS/LAZ-import of een import uit de KickTheMap-joblijst bewaart SleufBase voortaan **een volledige LAS/LAZ-kopie plus de Potree-conversie** in de beheerde `%LOCALAPPDATA%`-map, per scan-ID.
- Bij het openen en verder bewerken van een nieuwe scan wordt de interne AppData-kopie gebruikt. Het oorspronkelijke externe LAS/LAZ-bestand kan later verplaatst worden zonder dat de ingeladen scan verdwijnt.
- Selecties verwijderen uit de puntenwolkenlijst verwijdert, na bevestiging, zowel het beheerde LAS/LAZ-bestand als de 3D-conversie en aanvullende appgegevens. Het oorspronkelijke externe bestand blijft ongemoeid.
- Bij een bestaande, opnieuw gekoppelde scan wordt het bronbestand na identiteitscontrole ook in de beheerde map opgenomen.
- Onder **Instellingen → Puntenwolken** is het standaardcoördinatenstelsel instelbaar. De standaard is **RD New (EPSG:28992)**, ook voor KickTheMap-jobdownloads. WGS 84, ander/lokaal en metadata-afhankelijk zijn daarnaast beschikbaar. Deze instelling geldt voor nieuw te importeren scans; bestaande bronnen houden hun ingestelde CRS.
- Een geselecteerd CRS past de oorspronkelijke XYZ-coördinaten niet aan. Als het stelsel strijdig is met expliciete LAS/LAZ-EPSG-metadata weigert SleufBase de import.

## Uitgifte
- Release v1.0.61 is gemaakt zonder volledige automatische test- en Clippy-ronde. De geoptimaliseerde Windows-build en packaging-/checksumcontrole blijven vereist voordat GitHub de ZIP en installer publiceert.
