# SleufBase Rust v1.0.56 — offline luchtfoto als losse achtergrond en back-up standaard uit

## Kaartlagen en kadastrale achtergrond

- **Geen oude, lege kadastrale kaart bij het inzoomen:** de zelfstandige kadastrale kaart schakelt direct over naar de WFS-objectenweergave op perceelschaal. Een eerdere rasterkaart zonder objecten wordt daar niet meer achter de nauwkeurige kadastrale objecten getekend. Een tijdelijke WFS-storing veroorzaakt geen terugkeer van die oude rasterlaag; op grotere schalen blijft de bestaande kadastrale WMTS-kaart beschikbaar.
- **Offline luchtfoto als aparte achtergrondkeuze:** selecteer **Offline luchtfoto** bij de werkkaart of via Instellingen → Kaarten. Deze keuze gebruikt de ingebouwde landelijke overzichtsfoto en reeds opgeslagen detailtegels, en start zelf geen online kaart-WMTS/WMS/WFS-aanvraag.
- **Live kaartopties ongewijzigd:** de PDOK-luchtfoto, PDOK-luchtfoto + percelen, OpenStreetMap en OpenStreetMap + percelen blijven beschikbaar. De luchtfoto wordt nog steeds over de offline onderlaag gerenderd zolang de live luchtfoto gekozen is.

## Instellingen: expliciete toestemming en instelbaar detailniveau

- **Automatische download standaard UIT:** een uitgebreide offline luchtfotoback-up van 50 MB wordt niet meer automatisch bij de eerste start opgebouwd voor nieuwe installaties en na deze update, tenzij je dit zelf inschakelt. De kleine ingebouwde PDOK-foto blijft altijd offline beschikbaar.
- **Opt-in voor halfjaarlijkse updates:** wanneer de optie 'Automatisch een gedetailleerde offline luchtfoto downloaden en elke 6 maanden vernieuwen' is aangevinkt, worden ontbrekende tegels opgehaald en tegels van ten minste 183 dagen oud op de achtergrond vernieuwd. De update is annuleerbaar als je de optie weer uitzet.
- **Drie werkelijke detailniveaus:** **Laag (10 MB)** vraagt 768×768-pixeltegels aan; **Normaal (25 MB)** gebruikt 1024×1024; **Hoog (50 MB)** gebruikt 1536×1536. Dit zijn maxima per afzonderlijke cache voor 144 JPEG-tegels, zowel voor formaat als JPEG-compressie.
- **Voorkeuren worden onthouden:** de nieuwe instellingen worden in het bestaande Rust-instellingenbestand opgeslagen en hebben veilige standaardwaarden voor reeds bestaande installaties. Het ingeschakelde detailniveau is een algemene voorkeur, niet gebonden aan een projectbestand.
- **Bestaande data blijft behouden:** een al opgebouwde 50MB-cache blijft leesbaar; oudere caches van andere ingestelde detailniveaus worden niet automatisch gewist. Ook na uitschakelen kun je opgeslagen offlinebeelden gebruiken. De continue kleine overzichtsfoto is nooit afhankelijk van een netwerkverbinding.

De bronfoto komt van het Samenwerkingsverband Beeldmateriaal (CC BY 4.0), met zichtbare bronvermelding. De kaart ligt in RD / EPSG:28992. De DXF-export en andere kaartproviders worden niet gewijzigd.

## Build en validatie

Op verzoek worden de automatische tests en Clippy in deze release overgeslagen. De Windows-releasecompilatie en de pakket- en checksumcontroles worden wel uitgevoerd. Nieuwe regressietests staan in de code voor de opt-ininstellingen, detailniveaus, offline kaartkeuze en het verwijderen van de lege kadastrale rasterkaart; die tests zijn niet uitgevoerd.
