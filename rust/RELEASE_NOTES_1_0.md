## SleufBase Rust 1.0.21 voor Windows

### Variant-keuzepijl binnen het invoervak

- In het gecombineerde DXF-sjabloonexportvenster krijgt de keuzepijl voor **Variant** eerst de ruimte die de knop werkelijk nodig heeft. Het tekstveld gebruikt de resterende breedte van dezelfde rij.
- Daardoor overlapt de pijl de verticale scrollbar van het instellingenpaneel niet meer, ook wanneer het venster smaller wordt gemaakt.
- Eigen varianttekst, de standaardvarianten en de optie **Automatisch (B/C/D...)** blijven beschikbaar.

### DXF-sjabloonvensters verplaatsen

- Beide DXF-sjabloonexportvensters zijn weer aan de titelbalk te verslepen.
- Het vaste middenanker is vervangen door een eenmalige beginpositie. Daarna blijft de gekozen positie bruikbaar tijdens verplaatsen en vergroten of verkleinen.
- De begrensde inhoudsindeling uit versie 1.0.20 blijft behouden, zodat overlopende inhoud het venster niet steeds groter maakt. De vensters blijven begrensd tot het scherm.

### Publicatie zonder tests

Op verzoek zijn voor deze release geen tests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
