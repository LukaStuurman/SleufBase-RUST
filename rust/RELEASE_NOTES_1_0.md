# SleufBase Rust v1.0.52 — correcte straatnaamrotatie en kadastrale kaart bij ver uitzoomen

## Kadastrale kaart en '+ percelen'

- **Straatnamen volgen hun echte richting:** de interactieve Kadaster-WFS-kaart gebruikt nu de aangeleverde rotatiehoek van de straatnaam, in plaats van alle namen horizontaal te tekenen. De tekst blijft gecentreerd op het oorspronkelijke RD-labelpunt en een omgekeerde hoek wordt voor leesbaarheid genormaliseerd.
- **Ook op luchtfoto + percelen en OpenStreetMap + percelen:** dezelfde interactieve WFS-labelweergave wordt gebruikt voor de bijbehorende ingeschakelde perceelachtergrond. De labels volgen de oriëntatie die PDOK per openbare ruimte levert.
- **Geen harde uitschakeling meer bij 250 meter per pixel:** de kadastrale WMTS-ondergrond blijft ook bij verder uitzoomen ingeschakeld. De actuele WMTS-zoomniveaus bepalen de beschikbare detaillering.
- **Nederlandse kaart blijft zichtbaar in grote uitsneden:** als een ver uitgezoomde kaart buiten het RD-dekkingsgebied komt, wordt alleen het geldige Kadaster-deel opgehaald, pixelvast op de juiste positie geplaatst en wordt de ruimte buiten dat bereik neutraal afgebeeld. Dit voorkomt fouten voor de hele achtergrond.
- **Betere terugval bij ' + percelen':** wanneer de aanvullende transparante perceel-WMS op grote schaal niet beschikbaar is, blijft de onderliggende PDOK-luchtfoto of OpenStreetMap gewoon zichtbaar in plaats van dat de volledige achtergrond mislukt.
- **Export ongemoeid:** de DXF- en kaart-export blijven hun bestaande nauwkeurige kaartuitsnedes en labelrotaties gebruiken. De eerdere WMTS-snelheidsverbeteringen uit v1.0.50 en v1.0.51 blijven behouden.

Op zeer kleine kaartschalen kan de bron zelf minder perceelgrenzen en straatnamen tonen; de kaart blijft dan wel zichtbaar. De eigenschappen van de actuele PDOK-bron zijn bepalend.

## Build en validatie

De Windows-app wordt op verzoek zonder volledige geautomatiseerde tests en zonder Clippy gebouwd. De optimale releasecompilatie, het installatieprogramma, de portable ZIP, controles op de verpakking en checksums blijven vereist. Er zijn gerichte regressiecontroles aan de code toegevoegd voor rotatie en landelijke clipping, maar deze worden in deze release niet uitgevoerd.
