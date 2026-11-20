# SleufBase Rust v1.0.55 — scherpere offline luchtfoto tot circa 50 MiB

## Offline PDOK-overzicht en snelle kaart

- **Lage-resolutie overzicht blijft direct beschikbaar:** de echte gegeorefereerde PDOK-luchtfoto van 1536 × 1536 pixels zit nog altijd in het programma. Deze wordt meteen als onderlaag getoond wanneer scherpe luchtfototegels nog niet geladen zijn, ook zonder internet.
- **Maximaal 50 MiB aan lokaal opgeslagen detailbeelden:** SleufBase bouwt automatisch een landelijke overzichtscache met 144 gegeorefereerde JPEG-tegels (12 × 12) bij het starten. Elke tegel wordt apart gecomprimeerd en krijgt een vaste maximale bestandsgrootte. Het volledige detailarchief blijft binnen 50 MiB.
- **Meer detail zonder gigantisch GPU-geheugenverbruik:** elke tegel komt van de officiële PDOK WMS, tot 1536 × 1536 pixels. De applicatie verwerkt bestanden buiten de UI-thread en laadt maximaal achttien zichtbare of nabije tegels als GPU-textuur; de andere tegels blijven op schijf.
- **Aanmaken bij de eerste start:** het achtergrondproces vult ontbrekende tegels geleidelijk en begint bij de omgeving die op de kaart bekeken wordt. De app blijft bruikbaar tijdens downloaden en kan de volgende keer voortbouwen op de reeds aanwezige tegelbestanden.
- **Automatisch na zes maanden vernieuwen:** de applicatie controleert wanneer elke tegel voor het laatst is opgeslagen. Tegels vanaf 183 dagen oud worden tijdens gebruik automatisch opnieuw opgehaald. Bij een openblijvende app wordt ongeveer dagelijks opnieuw gecontroleerd.
- **Veilige foutafhandeling:** de oude afbeelding blijft zichtbaar totdat een nieuwe JPEG volledig is gedownload, gecontroleerd en atomair opgeslagen. Tijdelijke netwerkfouten leiden niet tot verlies van de bestaande offline achtergrond. Als downloaden mislukt probeert de app later opnieuw.
- **Blijft gekoppeld aan RD EPSG:28992:** de offline-tegels zijn geografisch uitgelijnd met de scherpe WMTS-luchtfoto en volgen dezelfde kaartcamera tijdens in-/uitzoomen en schuiven.
- **Alleen in PDOK-luchtfotomodus:** de onderlaag wordt gebruikt voor de interactieve PDOK-luchtfoto en PDOK-luchtfoto + percelen. OSM, de zelfstandige kadastrale kaart en de DXF-sjabloonexport worden niet aangepast.
- **Bronvermelding en licentie:** Samenwerkingsverband Beeldmateriaal, luchtfotodata onder CC BY 4.0; de bronvermelding staat zichtbaar op de kaart.

De cache wordt niet meteen 50 MiB groot; het daadwerkelijke volume hangt af van de JPEG-inhoud en hoeveel tegels al gedownload zijn. De 50 MiB is de **maximale** grootte, geen vereiste bestandsgrootte. Voor het aanvullen en verversen van de gedetailleerde offlinekaart is internet nodig; de kleine ingebouwde foto werkt zonder internet.

## Build en validatie

Zoals bij de vorige releases worden de volledige geautomatiseerde tests en Clippy op verzoek overgeslagen. De Windows-compilatie, portable ZIP, installer, pakketvalidatie en checksums blijven onderdeel van de releaseworkflow. De werking bij het eerste starten en bij offline gebruik is nog niet in een echte Windows-sessie gemeten.
