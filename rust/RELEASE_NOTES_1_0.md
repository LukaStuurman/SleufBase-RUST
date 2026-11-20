# SleufBase Rust v1.0.49 — PDOK-luchtfoto sneller zichtbaar tijdens zoomen en verschuiven

## Verbeteringen aan de interactieve luchtfotokaart

- **Sneller eerste beeld:** bij een koud kaartgebied mogen maximaal vier grovere WMTS-tegels worden opgehaald als tijdelijke achtergrond, naast de scherpe tegels. Daardoor hoeft de kaart niet op alle scherpe tegels te wachten voordat er überhaupt een luchtfoto zichtbaar wordt.
- **Geen blokkerende grove fase:** de grovere tegels en de detailtegels worden afgewisseld binnen de bestaande limiet van vier gelijktijdige PDOK-verzoeken. De gedetailleerde beelden blijven doorladen.
- **Minder witte vlakken:** al gecachete tegels van meerdere zoomniveaus vullen ontbrekende delen in. Het eerdere kaartbeeld blijft behouden waar het geografisch nog aansluit.
- **Scherpte behouden:** de gedownloade detailtegels worden over voorlopige beelden gelegd. Bij het verschijnen van een grove tegel worden de reeds aanwezige scherpe tegels niet overschreven.
- **Kleinere downloads:** de WMTS-luchtfoto wordt als JPEG aangevraagd in plaats van PNG. Eerder opgeslagen JPEG- en PNG-tegels blijven leesbaar.
- **Robuuster laden:** een fout bij een optionele grove previewtegel stopt het ophalen van de scherpe kaart niet. De bestaande WMS-terugval blijft beschikbaar als WMTS niet werkt.
- **Gerichte gedeeltelijke beeldupdates:** alleen het veranderde beeldgebied wordt opnieuw opgebouwd.

De DXF-sjabloonexport blijft ongewijzigd en behoudt de bestaande WMS-kaartuitsnede. Bij een volledig nieuw gebied blijft enige netwerkvertraging mogelijk; de werkelijke snelheidswinst is nog niet op een Windows-werkplek gemeten.

## Bouw en validatie

Deze versie wordt op verzoek **zonder geautomatiseerde tests en zonder Clippy** gebouwd. De noodzakelijke Windows-releasecompilatie, portable ZIP, installer, bestandsintegriteitscontrole en checksums worden wel uitgevoerd. De wijzigingen zijn daarom nog niet functioneel getest.
