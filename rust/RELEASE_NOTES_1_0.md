## SleufBase Rust 1.0.20 voor Windows

### Stabiele afmetingen van bewerkvensters

- **Proefsleuf bewerken** gebruikt nu een begrensd inhoudsvak dat exact overeenkomt met de gekozen venstermaat. De binnenliggende formulieren, kaders en scrollvakken kunnen de gewenste venstergrootte niet meer bij elke tekenronde vergroten.
- Het venster blijft met de muis te verplaatsen en te vergroten of verkleinen. De inhoud blijft scrollbaar en de actieknoppen blijven onderaan.
- Dezelfde begrenzing is toegepast op beide DXF-sjabloonexportvensters, waar dezelfde maatberekening stond.
- Een eerder te groot geworden editor wordt bij openen begrensd tot de beschikbare schermruimte.

### Terug naar het hoofdscherm na backupherstel

- Na het succesvol laden en overnemen van een backup sluit **Instellingen** meteen.
- Dit geldt zowel voor native SleufBase-backups als voor ondersteunde oudere Python-backups.
- Bij annuleren, een laadfout of een afgewezen verouderd laadresultaat blijft Instellingen beschikbaar. De bestaande bevestiging voor niet-opgeslagen projectwijzigingen blijft actief.

### Uitlijning van KickTheMap-keuzelijsten

- De eigen keuzelijsten voor kabel-/leidingtype en materiaal hebben compacte linkerpadding en links uitgelijnde tekst.
- De lege strook links van de opties wordt verwijderd, ook bij onbekende bestaande waarden en de lege keuze.
- De native keuzelijst, toetsenbordbediening en koppeling met de KickTheMap-velden blijven behouden.

### Compacte bovenmenu's

- De breedte van de menu's bovenaan volgt de langste menutekst met een kleine marge, in plaats van de volledige standaardbreedte van het popupvenster.
- De zichtbare scheidingslijnen tussen groepen en **Zoom naar extent** blijven behouden.

### TIFF-zichtbaarheidsschuif verwijderd

- De globale TIFF-zichtbaarheidsschuif is verwijderd uit de kaartbalk en de oudere weergave-invoer.
- Zichtbare TIFF-lagen worden op de interactieve kaart volledig dekkend getoond, zodat een oude schuifwaarde geen onbereikbare transparantie achterlaat.
- De afzonderlijke zichtbaarheid van proefsleuven en lagen blijft bedienbaar. Exportinstellingen en bronrasters worden niet gewijzigd.

### Publicatie zonder tests

Op verzoek zijn voor deze release geen tests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
