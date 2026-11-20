# SleufBase Rust v1.0.51 — minder witte luchtfotovakken bij uitzoomen en verschuiven

## PDOK WMTS: nabije gebieden alvast voorbereiden

- **Vooraf geladen randen:** nadat de zichtbare luchtfoto is geladen, bereidt SleufBase een kleine ring met grovere WMTS-tegels buiten het huidige kaartbeeld voor. Daardoor kan het programma bij een volgende zijwaartse beweging vaker meteen een luchtfoto in plaats van een wit vlak tonen.
- **Sneller uitzoomen:** een extra grof zoomniveau wordt mee opgeslagen om een volgende zoomstap te ondersteunen. Ook kaartuitsneden met minder dan acht scherpe tegels mogen nu een grovere tijdelijke voorbeeldkaart aanvragen.
- **Zichtbare kaart heeft voorrang:** maximaal zestien nabije grovere tegels per geplande voorlaadbeurt, één achtergronddownload tegelijk, en een lopende voorlaadbeurt wordt geannuleerd zodra de gebruiker een nieuw kaartgebied opvraagt. Bestaande scherpe tegeldownloads gebruiken de normale prioriteit en vierverzoeklimiet.
- **Direct uit geheugen:** de grovere tegels worden in een kleine gedeelde RAM-cache bewaard en via de bestaande schijfcache hergebruikt. Zo hoeft een net voorladen bestand niet eerst volledig van schijf terug te worden gelezen.
- **Geen beeldverlies bij een snelle beweging:** een geldige voorlaadtegel die nét klaar is op het moment van een nieuwe camerabeweging, blijft beschikbaar voor later gebruik.
- **Bestaande verbeteringen behouden:** doorlopend scherpere tegels tonen, doorlopende gedeeltelijke GPU-updates, automatisch opnieuw proberen na een fout, JPEG-downloads en RD-uitlijning blijven behouden.

Bij het allereerste bezoek aan een geheel nieuw gebied kunnen nog kort witte delen optreden: daarvoor moet ten minste één echte luchtfototegel via internet binnenkomen. De nieuwe aanpak minimaliseert dit vooral voor aangrenzende en recent bezochte gebieden, zonder andere PDOK-kaarten massaal binnen te halen.

## Build en validatie

De release wordt op verzoek **zonder geautomatiseerde tests en zonder Clippy** gebouwd. De Windows-releasecompilatie en controle van het installatieprogramma, de portable ZIP en checksums worden wel uitgevoerd. De daadwerkelijke respons op een Windows-werkplek is niet gemeten.
