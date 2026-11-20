# SleufBase Rust v1.0.60 — vector-Kadaster, offline kaarten en puntenwolken

## Kadastrale kaart zonder raster

- De interactieve kadastrale kaart gebruikt alleen PDOK WFS-vectorgegevens voor perceelgrenzen, bebouwing en labels. De oude WMTS/WMS-kadastrale rasterkaart is geen terugvaloptie meer.
- PDOK luchtfoto + percelen en OpenStreetMap + percelen combineren de normale achtergrond met echte kadastrale vectorgeometrie. Bij ver uitzoomen kan de WFS-laag wegens de service- en prestatiegrenzen ontbreken; de oude kadastrale rasterlaag wordt niet hersteld.
- Standalone kaart- en sjabloonexport tekent kadastrale vectorlijnen en labels rechtstreeks op de gekozen uitvoerresolutie. Een te grote puur kadastrale vectoruitsnede geeft een duidelijke melding.

## Offline luchtfoto op originele resolutie

- Onder **Instellingen → Kaarten** kan het downloadgebied worden gekozen: **Heel Nederland** of **Huidige kaartuitsnede**. Voor grote gebieden is een lager WMTS-zoomniveau nodig; de app verlaagt de resolutie nooit stilzwijgend.
- SleufBase downloadt uitsluitend de originele 256 × 256 JPEG-tegels van het geselecteerde hoogste zoomniveau (0 t/m 19). Er worden na afloop geen kleinere PNG/JPEG-cachebestanden aangemaakt. De ingebouwde landelijke overzichtsfoto blijft beschikbaar bij ver uitzoomen.
- Downloaden gebruikt maximaal vier gelijktijdige HTTP-verbindingen met korte retries, behoudt reeds geldige recente tegels en schrijft vervangingen atomair.
- **JPEG-compressie** kan worden gekozen: originele PDOK-JPEG zonder hercodering (100) of kwaliteitsniveau 95, 85 of 70. De breedte en hoogte van de tegel veranderen bij compressie niet.
- Gedownloade gebieden en hun opslaggrootte zijn zichtbaar in de instellingen. Afzonderlijke gebieden of alle offline caches (inclusief oude offlinekaarten) kunnen worden verwijderd. Gedeelde tegels blijven staan zolang een ander geregistreerd gebied ze nodig heeft.
- Verwijderen tijdens een actieve download wordt geblokkeerd. Bij verwijderen wordt de automatische vernieuwing uitgeschakeld om onmiddellijke herdownload te voorkomen.

## Puntenwolkeditor en puntenwolkenlijst

- **Zoomen en pannen nabij oppervlakken** behouden een bruikbare minimumsnelheid in de Potree-editor, zonder dat eerst dubbelklikken op een oppervlak nodig is.
- De puntenwolkenlijst kan één of meerdere opgeslagen scans verwijderen via een bevestigingsvenster. De oorspronkelijke LAS/LAZ-bestanden blijven op schijf. Lokaal opgebouwde Potree-data wordt op een achtergrondworker opgeruimd.
- Verwijderde lijstrecords worden niet automatisch opnieuw vanuit oude projectbestanden geïmporteerd. Al afzonderlijk opgeslagen proefsleufgeometrie blijft behouden.

## KickTheMap: puntenwolken van geselecteerde jobs downloaden

- In **SleufBase Jobs** staat een knop **Puntenwolken downloaden** voor de geselecteerde KickTheMap-jobs.
- Voor elke downloadbare job wordt een LAZ-export geprobeerd; indien die niet beschikbaar is, wordt de oudere LAS-export geprobeerd. Bestanden worden via de bestaande geauthenticeerde en gecontroleerde downloadroute opgehaald.
- Maximaal drie jobs worden tegelijk gedownload. De gedownloade LAS/LAZ-bestanden gaan daarna via de bestaande importwachtrij naar **Puntenwolken beheren**, zonder de scans automatisch in het huidige project te laden.

## Releaseproces

Deze uitgifte wordt op uitdrukkelijk verzoek gebouwd **zonder de gebruikelijke automatische test- en Clippy-stappen**. De geoptimaliseerde Windows-app, portable ZIP en installer worden wel samengesteld en op bestandsintegriteit gecontroleerd. De werkelijke werking met de gekoppelde KickTheMap-account, PDOK-services en de Windows/WebView2-editor is niet praktijkgetest in deze uitgifte.
