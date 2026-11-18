## SleufBase Rust 1.0.28 voor Windows

### Locatie openen en sluiten

- Klik nogmaals op **Locatie** om het zoekpaneel te sluiten.
- Bij sluiten worden een lopende zoekopdracht en nog geplande zoeksuggesties geannuleerd. De zoektekst blijft bewaard voor wanneer je het paneel weer opent.
- **Ctrl+L** blijft het zoekpaneel openen en zet de cursor in het zoekveld, ook als het paneel al openstaat.

### TIFF-adressen passen zich aan het venster aan

- De adressenlijst gebruikt de beschikbare vensterhoogte, zodat een groter venster meer regels toont.
- Kolommen passen zich aan de breedte aan. In een smal venster staan bestandsnaam en adres onder elkaar.
- Kopieerknoppen en **Vernieuwen** blijven onderaan beschikbaar en gaan bij weinig ruimte naar een volgende regel.
- De uitleg in **Info en handleiding** is bijgewerkt.

### Publicatie zonder tests

Op verzoek zijn geen tests, Clippy-controles of applicatietests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
