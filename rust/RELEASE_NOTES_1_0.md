# SleufBase Rust 1.0.36 — GeoTIFF-oriëntatie en directe Potree-typekleuren

## Verbeteringen

- Een GeoTIFF-bovenaanzicht van een LAS/LAZ-puntenwolk krijgt zijn kijkrichting van het geplaatste **beginpunt naar eindpunt**. Die richting loopt naar rechts in het beeld. De GeoTIFF gebruikt een volledige affine RD New (EPSG:28992) transformatiematrix, zodat het beeld geografisch correct blijft op de SleufBase-kaart en in de DXF-sjabloonexport.
- Een opgeslagen bovenaanzicht wordt alleen hergebruikt als de begin-/eindcoördinaten nog overeenkomen. Verplaats je begin of eind later, dan wordt het beeld bij opnieuw inladen opnieuw opgebouwd. Het bestaande raster wordt bij voltooiing met bijgewerkte transformatie en textuur herladen.
- Zonder geldig begin-/eindpaar blijft het bovenaanzicht voorlopig noordgericht. Een later toegevoegd paar bepaalt de nieuwe richting. Er worden geen editorannotaties in de scanachtergrond gefotografeerd.
- In de Potree-editor krijgen een **punt en polylijn onmiddellijk hun gekozen typekleur** wanneer het type wordt aangepast. De geselecteerde oranje markeerkleur maskeert de typekleur niet meer; de objectlijst houdt de selectie zichtbaar. Ook bestaande Potree-marker- en lijnmaterialen worden bijgewerkt.
- De verbeteringen van 1.0.35 blijven aanwezig: originele polylijnen in dwarsprofiel, TIFF-detail en kaart, directe inlaadfunctie, meerdere LAS/LAZ-scans in het overzicht en GeoTIFF uit bronpunten.

## Controle

GitHub bouwt de Windows-app en controleert de portable ZIP en installer voor publicatie. De volledige testset blijft volgens afspraak uitgeschakeld. Interactieve controle met het oorspronkelijke Windows-scanbestand is niet uitgevoerd.
