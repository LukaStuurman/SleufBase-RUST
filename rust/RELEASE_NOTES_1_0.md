# SleufBase Rust v1.0.48 — snellere dynamische PDOK-luchtfoto

## PDOK-luchtfoto sneller laden, zoomen en verschuiven

- **Scherpe tegels hebben voorrang:** de interactieve PDOK-luchtfoto vraagt de relevante WMTS-tegels op het juiste zoomniveau meteen aan, van het kaartmidden naar buiten. Een extra grof zoomniveau blokkeert niet langer de hoge-resolutietegels.
- **Slimmer gebruik van de cache:** al beschikbare grovere tegels en het eerder getoonde kaartbeeld kunnen tijdelijk als achtergrond dienen. Nieuwe detailtegels worden toegevoegd zonder eerst opnieuw een volledig achtergrondbeeld te downloaden.
- **Geen onnodige WMS-overscan:** voor de interactieve WMTS-luchtfoto wordt niet langer standaard een veel groter kaartgebied buiten het venster opgevraagd, wat het aantal benodigde tegels en de kans op een te laag zoomniveau vermindert.
- **Alleen gewijzigde pixels tekenen:** bij een ontvangen kaarttegel worden alleen het eigen zichtbare gebied en de bijbehorende bilineaire rand bijgewerkt.
- **Gedeeltelijke GPU-updates:** kleine, geverifieerde pixelrechthoeken worden als afzonderlijke textuurupdates verstuurd. Het eerste volledige beeld en de geografische RD-uitlijning blijven intact.
- **Minder herhaalde rasterberekeningen:** de uiteindelijke foto hoeft niet nog eens integraal te worden gesampled wanneer de detailtegels al op hun juiste positie staan.
- **Sneller omgaan met scrollen:** afgeronde downloads gaan direct naar de geheugencache, ook wanneer de gebruiker ondertussen een andere uitsnede kiest. Deze tegels zijn later opnieuw te gebruiken.
- **Asynchrone schijfcache:** downloads hoeven niet te wachten op het wegschrijven van afbeeldingen. De cache blijft begrensd.
- De interactieve kaart behoudt de bestaande WMS-terugval wanneer WMTS niet bruikbaar is. DXF-sjabloonexport gebruikt nog altijd een exacte WMS-kaartuitsnede.

## Validatie en beperkingen

- Regressiecontroles voor RD-tegelposities, volledige/deelgewijze rasteruitvoer en gedeeltelijke pixelupdates zijn aan de broncode toegevoegd. Een eerdere PR-controle compileerde, maar de totale testsuite bevatte meerdere mislukte bestaande en aangepaste kaart-/UI-tests.
- **Deze release wordt op verzoek zonder geautomatiseerde tests en Clippy gebouwd.** De Windows-releasecompilatie, portable ZIP, installatieprogramma en controle van pakketbestanden en checksums worden wel uitgevoerd.
- De werkelijke snelheidswinst hangt onder andere af van schermresolutie, internetverbinding, GPU en de inhoud van de cache. Er is nog geen praktijkbenchmark op een Windows-werkplek uitgevoerd.
