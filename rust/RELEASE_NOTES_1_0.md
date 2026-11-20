# SleufBase Rust v1.0.58 — offline PDOK-luchtfoto op alle echte zoomniveaus

## Zoomniveaus, benodigde MB en gebied

- **Alle 20 officiële PDOK-WMTS-zoomniveaus:** onder Instellingen → Kaarten kies je nu niveau **0 t/m 19**. De opties tonen het echte aantal meters of centimeters per pixel, en voor de huidige kaartuitsnede bij elk niveau een schatting van de benodigde MB en aantal JPEG-tegels.
- **Geen kunstmatige kaartgrootte van 50 MB meer:** er wordt niet langer geprobeerd heel Nederland in een vaste JPEG-limiet van 10, 25 of 50 MB te persen. Elk gekozen niveau haalt de echte 256×256-pixeltegelbeelden uit de PDOK WMTS-laag `Actueel_orthoHR`.
- **Kosten afhankelijk van oppervlak:** de schatting is gebaseerd op ongeveer 50 KiB per JPEG-tegel, het huidige RD-kaartgebied en alle zoomniveaus van 0 tot het gekozen maximum. Werkelijke bestandsgrootte verschilt per luchtfoto, locatie en JPEG-compressie. Zonder geladen kaart wordt expliciet een **voorbeeldgebied van 1 × 1 km** gebruikt voor de schatting.
- **Klein gebied voor hoge resolutie:** bij hoge zoomniveaus kost een landelijke kaart vele gigabytes of meer. De downloadknop accepteert daarom alleen kaartuitsneden tot maximaal 4.096 tegels per opdracht; bij een grotere selectie verschijnt een waarschuwing om verder in te zoomen of een lager niveau te kiezen. De niveaus blijven wel allemaal zichtbaar en selecteerbaar.
- **Native bronresolutie:** niveau 15 is ongeveer 10,5 cm/pixel en niveau 16 circa 5,3 cm/pixel. Niveaus 17 t/m 19 zijn op PDOK technisch opvraagbaar, maar leveren meestal geen werkelijk extra informatie boven de huidige bronresolutie van 5–8 cm/pixel.

## Downloaden, automatisch vernieuwen en kaartweergave

- **Eenmalige knop:** `Offline luchtfoto nu downloaden en gebruiken` legt de actuele kaartuitsnede en het gekozen WMTS-niveau vast, bouwt de niveaus van 0 tot en met het maximum op en schakelt de werkkaart direct naar de offline achtergrond. De automatische zesmaandelijkse download blijft standaard uit.
- **Halfjaarlijks verversen per gekozen gebied:** als deze optie bewust wordt aangezet, onthoudt de app het kaartgebied plus zoomniveau en vernieuwt na 183 dagen verouderde WMTS-JPEG-tegels. Bij nog geen gekozen gebied kan de actuele kaartuitsnede na Opslaan worden vastgelegd.
- **Permanente offline fallback:** de kleine meegeleverde gegeorefereerde Nederland-luchtfoto blijft beschikbaar, net als eerder opgeslagen 10/25/50MB-previewbestanden. De nieuwe WMTS-laag wordt daaroverheen getekend, zodat ook tijdens het eerste downloaden grovere niveaus zichtbaar kunnen zijn.
- **Begrensd GPU-geheugen:** de offline kaart laadt alleen maximaal enkele tientallen 256px-tegels rond de huidige kaartcamera, met een beperkte geheugen-LRU. De volledige offline tegelverzameling wordt nooit als één enorme GPU-textuur ingelezen.
- **Veilig downloaden en helder resultaat:** netwerkverbindingen zijn begrensd, foutieve JPEG's worden geweigerd, schrijven gebeurt atomair en een mislukte download verwijdert geen bestaande tegels. De voortgang blijft binnen Instellingen en de hoofdstatusbalk blijft stil bij normale kaartlaadacties.
- **Licentie:** luchtfoto van het Samenwerkingsverband Beeldmateriaal onder CC BY 4.0 met zichtbare bronvermelding.

## Validatie

De Rust-bron bevat controles voor alle 20 niveaus, gebiedsgrenzen, tegeluitlijning, geschatte bestandsgrootte en een eenmalige download zonder automatische opt-in. De Windows-release wordt volgens de eerdere afspraak gebouwd zonder volledige geautomatiseerde tests en Clippy. Compilatie en uitgifte van de installer, portable ZIP en SHA-256-checksums worden wel uitgevoerd. De werkelijke laadsnelheid en opslagomvang moeten nog op een Windows-werkplek worden bevestigd.
