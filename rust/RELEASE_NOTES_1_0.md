# SleufBase Rust v1.0.50 — PDOK WMTS blijft doorladen zonder de kaart te bewegen

## Geen stilstaande scherpe beelden en minder witte blokken

- **Voortdurend scherpere tegels tonen:** iedere binnengekomen PDOK WMTS-tegel wordt direct als deelupdate aangeboden. De eerdere beperking van 2 tot 5 tussentijdse beelden is verwijderd.
- **Geen verloren tegelupdates:** de achtergrondwerker bewaart alle nog niet getoonde pixelrechthoeken tot de gebruikersinterface ze heeft verwerkt. De vorige grens van 16 patches kon reeds gedownloade tegels laten verdwijnen.
- **Automatisch herstellen zonder beweging:** als een kaartaanvraag mislukt, probeert SleufBase het opnieuw terwijl de kaart stilstaat. Opeenvolgende fouten krijgen een oplopende wachttijd, zodat de PDOK-dienst niet onnodig wordt belast.
- **Meer resultaat bij individuele netwerkfouten:** als één detailtegel mislukt, kunnen de overige tegels uit dezelfde aanvraag nog worden ontvangen en op de kaart verschijnen, voordat de bestaande WMS-terugval wordt gebruikt.
- **Behoud van eerdere versnellingen:** JPEG-tegels, grovere voorbeeldtegels, geheugencache, schijfcache, RD-positionering en gedeeltelijke GPU-textuurupdates blijven behouden.

Een volledig onbekend gebied kan nog kort wit zijn voordat de eerste netwerkbeelden beschikbaar zijn. De werkelijke snelheidswinst en het gedrag op jouw Windows-computer zijn niet in een praktijksessie gemeten.

## Validatie en release

Op verzoek worden de geautomatiseerde tests en Clippy net als bij de vorige release overgeslagen. Er zijn wel extra regressietests aan de broncode toegevoegd voor een wachtrij met meer dan 16 patches en automatisch opnieuw proberen na een mislukte aanvraag. De Windows-build, portable ZIP, installatieprogramma, pakketcontroles en checksums worden door de releaseworkflow uitgevoerd.
