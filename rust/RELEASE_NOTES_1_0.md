# SleufBase Rust v1.0.54 — een echte offline luchtfoto onder snelle WMTS-tegels

## Witte stroken tijdens snel uitzoomen en slepen

- **Landelijke luchtfoto ingebouwd in het programma:** een kleine, echte PDOK-luchtfoto van 1536 × 1536 pixels is onderdeel van de Windows-app (ongeveer 250 KB JPEG op schijf). Deze is bij de eerste start meteen beschikbaar, ook zonder internet. De foto is gegeorefereerd in RD (EPSG:28992), met een vaste dekking van -100.000 tot 400.000 X en 200.000 tot 700.000 Y.
- **Scherpe kaart over offline kaart:** de lage-resolutiefoto is een permanente onderste kaartlaag voor de PDOK-luchtfoto en de PDOK-luchtfoto + percelen. Bestaande scherpe WMTS-tegels komen eroverheen, zonder een extra aanvraag te blokkeren.
- **Geen ondoorzichtig witte ontbrekende WMTS-tegels:** de renderer laat nog niet geladen pixels transparant totdat een echte tegel beschikbaar is. Hierdoor wordt op zulke plekken tijdelijk de offline luchtfoto zichtbaar in plaats van een wit blok.
- **Automatische halfjaarlijkse update:** als de ingebouwde of laatst gedownloade overzichtsfoto minimaal 183 dagen oud is, haalt SleufBase bij gebruik automatisch op de achtergrond een nieuwe versie van de officiële PDOK WMS (Actueel_orthoHR). Ook wanneer de app lang geopend blijft wordt hierop gecontroleerd.
- **Betrouwbare updates:** er gelden tijdslimieten, een maximale bestandsgrootte, controle van het JPEG-formaat en vaste afbeeldingsafmetingen. Pas na succesvolle download en validatie vervangt een atomaire bestandswrite de oude versie. Bij een storing blijft de ingebouwde of eerder opgeslagen kaart beschikbaar; een volgende poging volgt later.
- **Scherpe beelden behouden:** de eerdere verbeteringen aan het laden, de vooraf geladen kaartmarge en het halfjaarlijks bijwerken van de lage-resolutie onderlaag beïnvloeden de coördinaten en resolutie van de scherpe PDOK-kaart niet.
- **Bronvermelding zichtbaar op de kaart:** luchtfoto's van het Samenwerkingsverband Beeldmateriaal worden beschikbaar gesteld als open data onder [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). De bron staat vermeld bij de kaart en in de broncode.
- **Geen wijziging aan DXF-sjabloonexport of andere kaartproviders:** het gaat om de interactieve PDOK-luchtfoto.

De offline fotodekking is begrensd tot het Nederlandse kaartgebied en de directe omgeving. Bij het zoomen naar een compleet andere regio buiten die dekking kan een neutrale achtergrond zichtbaar blijven. De exacte prestaties bij zeer snel slepen of uitzoomen moeten nog op de Windows-werkplek worden beoordeeld.

## Releaseprocedure

De Windows-release wordt op verzoek zonder geautomatiseerde tests en Clippy gebouwd. De geoptimaliseerde compilatie, portable ZIP, installer, pakketcontroles en checksums worden wel uitgevoerd. Voor de nieuwe module zijn enkele regressiecontroles toegevoegd, maar ze worden niet uitgevoerd in de releaseworkflow.
