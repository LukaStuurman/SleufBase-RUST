## SleufBase Rust 1.0.9 voor Windows

### Naambordjes op de werkkaart

- Ook gewone GeoTIFF- en KickTheMap-proefsleuven krijgen een naambordje. MarXact- en virtuele proefsleuven gebruiken dezelfde verzorgde stijl met afgeronde bordjes, een rustige achtergrond en een herkenbaar kleuraccent.
- KickTheMap gebruikt de jobnaam als die beschikbaar is; losse TIFF-bestanden gebruiken de bestandsnaam zonder extensie. MarXact- en virtuele sleuven gebruiken hun sleufnaam.
- Bordjes blijven leesbaar bij in- en uitzoomen en zoeken vrije ruimte buiten TIFF-beelden, sleufgrenzen en andere bordjes. Wanneer er onvoldoende vrije ruimte op het scherm is, wordt het bordje tijdelijk verborgen.
- Via **Instellingen → Kaarten → Kaartscherm → Naambordjes van proefsleuven tonen** kun je alle naambordjes aan- of uitzetten. Klik op **Opslaan** om de keuze toe te passen en te onthouden voor volgende sessies.
- De naambordjes zijn uitsluitend onderdeel van de interactieve werkkaart. Ze worden niet toegevoegd aan kaart- of DXF-sjabloonexports.

### Meerdere lagen tegelijk selecteren en verwijderen

- De lijsten heten voortaan **Proefsleuven** en **Overlays** en hebben dezelfde rustige, lichte vormgeving.
- Ook TIFF-, virtuele en MarXact-proefsleuven hebben een zichtbaarheidsvakje. Een verborgen sleuf verdwijnt met zijn naambordje en bewerkingsmarkeringen uit de werkkaart. Sla het project op om de zichtbaarheid te bewaren.
- De scrollbalken in beide lijsten zijn breder en hebben ook zonder aanwijzen of klikken een duidelijke, contrastrijke schuif en baan.
- Houd **Ctrl** ingedrukt om losse items aan de selectie toe te voegen of eruit te halen. Met **Shift** selecteer je een aaneengesloten reeks; **Ctrl+Shift** voegt een reeks toe aan de bestaande selectie.
- Verwijder de geselecteerde items samen via de verwijderknop, het contextmenu of de Delete-toets terwijl de lijst actief is.
- De selectie werkt ook met een combinatie van TIFF- en virtuele/MarXact-proefsleuven, en met objectlagen en DXF-overlays. Alleen de lagen in het project worden verwijderd; de bronbestanden blijven behouden.

### Info en uitleg per onderdeel

- Naast **marXact** staat een nieuwe knop **Info**.
- Het infovenster bevat doorzoekbare uitleg per onderwerp, waaronder de eerste stappen, kaartbediening, KickTheMap, GeoTIFF/KLIC, MarXact, virtuele proefsleuven, referentiepunten, opmerkingen, exports, instellingen en projectopslag.
- De vermelding **Gemaakt door Luka Stuurman** en het actuele versienummer staan in het infovenster.

### Kleinere vensters met voortgang

- De DXF-sjablooninstellingen openen in een kleiner venster in het midden van het scherm. De inhoud kan scrollen; de actieknoppen blijven bereikbaar.
- Langlopende taken, waaronder beginpunten laden, bestanden laden en DXF-sjabloonexport, gebruiken een compact gecentreerd voortgangsvenster met de werkkaart op de achtergrond.
- De voortgangsbalk toont aantallen als de taak een bekend totaal heeft. Andere taken tonen een bewegende balk met de actuele bewerkingsstap. Annuleren blijft beschikbaar zolang de taak veilig kan worden gestopt.

### Build en release

De volledige geautomatiseerde tests en installatieproeven zijn conform de eerdere voorkeur overgeslagen. De Windows-app en installer worden gecompileerd; de releasebestanden worden gecontroleerd op versie, inhoud en SHA-256-controlesom voordat de release wordt gepubliceerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
