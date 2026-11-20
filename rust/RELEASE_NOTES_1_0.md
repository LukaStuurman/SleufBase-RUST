# SleufBase Rust v1.0.53 — geen witte kaartranden bij korte pan en zoom

## Renderer en verversing van PDOK-luchtfoto's

- **Renderbuffer rond de zichtbare kaart:** de interactieve PDOK-luchtfoto, ook in de variant met percelen, rastert nu ongeveer 30% buiten elk van de vier schermranden. Deze reserve is al een volledige beeldtextuur op de GPU, in plaats van alleen vooraf gedownloade tegels die pas na een nieuwe aanvraag zichtbaar worden.
- **Zelfde scherpte:** zowel de BBOX als het aantal afbeeldingspixels groeien mee. De resolutie in meters per pixel verandert niet door deze extra kaartmarge.
- **Verversen vóór het bereiken van de rand:** zodra de reserve onder ongeveer 12% van de zichtbare breedte of hoogte komt, vraagt de kaart tijdig een nieuwe gebufferde uitsnede aan. Dat vermindert witte stroken bij scrollen en zijwaarts slepen.
- **Begrensd geheugenverbruik:** de renderbuffer houdt rekening met maximaal 6 miljoen pixels voor interactieve PDOK-achtergronden. Grote schermen kunnen daarom terugvallen op een kleinere of exacte uitsnede.
- **Geen witte flits bij het aanzetten van percelen:** de huidige luchtfoto blijft als GPU-textuur bestaan wanneer de kaart wisselt tussen PDOK-luchtfoto en PDOK-luchtfoto + percelen. Hetzelfde geldt voor de overeenkomstige OpenStreetMap-varianten. Een bestaande Kadaster-WMTS-textuur blijft bij de overgang naar een zuivere WFS-verkavelingsweergave beschikbaar.
- **OpenStreetMap-gebruiksvoorwaarden:** OpenStreetMap haalt buiten de actieve viewport geen extra rastertegels vooraf op; de extra rastermarge geldt alleen voor PDOK. De bestaande caches, doorlopende tegelupdates en achtergrond-downloader blijven behouden.
- **DXF-export ongewijzigd:** de kaartbuffer is uitsluitend voor de interactieve kaartweergave.

Voor nooit eerder geladen gebieden kan de eerste internetdownload nog kort een lege achtergrond geven. Bij zeer snelle sprongen buiten de gehele cache is daadwerkelijke luchtfoto-inhoud niet onmiddellijk beschikbaar.

## Build en controle

De Windows-release wordt op verzoek zonder volledige geautomatiseerde tests en Clippy gebouwd. Er zijn wel broncodecontroles toegevoegd voor pixelresolutie, verversen met marge, providerwisselingen en geheugenlimieten. De releaseworkflow controleert de compilatie, portable ZIP, installer en checksums. Een praktische visuele proef op jouw Windows-pc is nog nodig.
