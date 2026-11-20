# SleufBase Rust v1.0.59 — hoogste offline PDOK-zoomniveau downloaden, zelf verkleinen bij uitzoomen

## Alleen de scherpste luchtfoto downloaden

- **Geen onnodige lagere PDOK-zoomniveaus meer downloaden.** SleufBase haalt voor de expliciet gekozen RD-kaartuitsnede uitsluitend de JPEG-tegels op van het gekozen hoogste PDOK WMTS-zoomniveau (0–19). Er worden geen netwerkverzoeken verstuurd om dezelfde luchtfoto nogmaals op lagere zoomniveaus op te halen.
- **Zelf kleinere kaartbeelden maken voor uitzoomen.** Na het opslaan van de hoogste kwaliteit maakt SleufBase op de achtergrond kleinere 256px-overzichtstegels door lokaal telkens vier scherpe tegels samen te voegen en te verkleinen. Dit levert de nodige tussenliggende schaalniveaus op zonder een nieuwe PDOK-download.
- **Transparante beeldranden:** buiten de opgeslagen kaartuitsnede blijven gegenereerde PNG-tegels doorzichtig, zodat de al ingebouwde landelijke overzichtsfoto zichtbaar blijft. Aanwezige eerdere offlinebeelden worden niet weggegooid.
- **Details blijven zichtbaar tijdens bewegen:** de renderer herkent de gegenereerde lokale PNG-overzichten en toont ze bij uitzoomen. De tegelkeuze herkent ook gedownloade randen als het midden van het scherm buiten de opgeslagen foto valt.
- **Geschatte MB is nu de échte netwerk-download.** In Instellingen → Kaarten tonen alle 20 PDOK-WMTS-zoomniveaus het geraamde aantal te downloaden **hoogste-resolutietegels** en circa MB (op basis van ca. 50 KiB per JPEG). Lagere beelden worden lokaal gemaakt en vragen wel extra schijfruimte, maar geen extra download.
- **Veilige opslag:** kleine voorbeelden worden lossless PNG met alfa opgeslagen onder de bestaande offline-cachemap. Zowel JPEG-foto's als de zelfgemaakte overzichten worden gecontroleerd, atomair gepubliceerd en bij de volgende start hergebruikt.
- **Ook bij deels mislukte downloads:** als enkele beelden niet bereikbaar zijn, worden lagere overzichten waar mogelijk alsnog opgebouwd uit de succesvol opgeslagen scherpe beelden. De gebruiker ontvangt een melding dat de download onvolledig is.
- **Handmatig en automatisch:** de knop voor direct downloaden en de optionele zesmaandelijkse verversing gebruiken allebei deze aanpak. Automatische downloads blijven standaard uit en de limiet van 4.096 scherpe PDOK-tegels per gekozen uitsnede blijft behouden.

## Compatibiliteit

Bestaande offlinekaarten en eventueel eerder op PDOK gedownloade lagere zoomniveaus blijven leesbaar; de nieuwe versie vraagt lagere niveaus niet meer bij PDOK op. De kleine ingebouwde Nederland-foto blijft de laatste offline fallback. De gewone live-PDOK-kaart, Kadasterachtergrond, andere databronnen en DXF-export zijn niet aangepast.

## Uitgifte

De release wordt zonder de volledige automatische test- en Clippy-ronde gebouwd, conform de vorige uitgiften. De geoptimaliseerde Windows-build en de portable ZIP, installer en checksums worden gecontroleerd. Een nieuwe regressiecontrole verifieert dat vier hoge-resolutiefoto's lokaal één correct geografisch uitgelijnde verkleinde tegel vormen. Praktische weergave op Windows blijft nog te beoordelen.
