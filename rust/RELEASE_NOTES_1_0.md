## SleufBase Rust 1.0.2 voor Windows

### Scherpere en duidelijkere DXF-sjabloonexport
- De luchtfotokaart in het DXF-sjabloon krijgt compactere witte randen, terwijl RD-coördinaten, schaalbalk, legenda en noordpijl behouden blijven.
- Nieuwe exports gebruiken standaard 300 DPI in plaats van 150 DPI: viermaal zoveel kaartpixels.
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
