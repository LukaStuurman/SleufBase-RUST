## SleufBase Rust 1.0.2 voor Windows

### Gemma verwijderd en proefsleufinvoer verbeterd

- De Gemma-AI-functie, het AI-menu en de bijbehorende modelruntime zijn verwijderd.
- Handmatige maaiveldteksten, lijnkleuren en AHN-aanvulling blijven beschikbaar.
- Een kabel of leiding die nog in het invoerveld staat, wordt nu ook meegenomen bij een handmatige kopie en bij kopiëren naar Excel.
- Excel-import maakt geen dubbele kabel meer nadat een bestaande kabel is geselecteerd.
- Bij het aanpassen van de beginhoogte worden alleen kabelhoogtes die expliciet aan het beginmaaiveld gekoppeld zijn opnieuw berekend. Hun ingevoerde diepte blijft behouden; gemeten en absolute hoogtes blijven intact.
- Geïmporteerde Python-projecten behouden hun oorspronkelijke kaartinstellingen en kunnen weer verliesvrij worden teruggeschreven. Bewaar bewust gekozen hogere resoluties in een Rust-project.
- De extra kabelaccenten zijn beperkt tot sjabloonexports. Gewone kaartuitvoer behoudt de gekozen rasterkwaliteit en de oorspronkelijke TIFF-weergave.
- Extra sjabloonmarkeringen volgen de ingestelde dekking en verdwijnen volledig wanneer de rasterlaag onzichtbaar is.
- Het versienummer van de Windows-installer komt overeen met de applicatieversie.

### Scherpere en duidelijkere DXF-sjabloonexport

- De luchtfotokaart in het DXF-sjabloon krijgt compactere witte randen, terwijl RD-coördinaten, schaalbalk, legenda en noordpijl behouden blijven.
- Nieuwe Rust-projecten gebruiken standaard 300 DPI in plaats van 150 DPI: viermaal zoveel kaartpixels.
- Virtuele proefsleuven worden voor de detailafbeelding met kwaliteitsfactor 4 gerenderd, wat ongeveer zestienmaal zoveel detailpixels geeft.
- Kabel- en leidingpunten van virtuele proefsleuven krijgen duidelijke, kleurgecodeerde markers met contrastrand op de luchtfoto, ook als de sleuf zelf erg klein in beeld is.
- De grote detailafbeelding wordt niet opnieuw in volledige resolutie opgebouwd voor de kaart, om geheugen en exporttijd te beperken.
- De zelfstandige A4-kaartenexport behoudt de oorspronkelijke paginamarges; de compactere opmaak geldt alleen voor DXF-sjablonen.

### Eerdere wijzigingen (v1.0.1)

- Eén uniform exportvenster voor KickTheMap-jobs, GeoTIFF- en virtuele proefsleuven, ongeacht de volgorde van toevoegen.
- Virtuele proefsleuven blijven in de selectielijst staan na het herstellen van oude sjablooninstellingen.
- Nieuwe online exports gebruiken standaard PDOK-luchtfoto's, offline exports blijven zonder online achtergrond.
- Native KickTheMap-inloggen met beveiligd opgeslagen aanmeldgegevens, betere GeoTIFF-coördinaten en stabiele kabeloriëntatie.

Gebruik de Windows-installer of portable ZIP. Vergelijk de downloads met de SHA-256-bestanden indien gewenst.
