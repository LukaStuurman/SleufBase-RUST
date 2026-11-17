## SleufBase Rust 1.0.25 voor Windows

### Overlappende kabelobjecten groeperen

- Wanneer je met de selectielijn meerdere kabels op dezelfde plek raakt, verschijnen die samen als één ingeklapte groep in **Kabelobjecten**.
- De groepskop toont het aantal objecten per laagnaam, bijvoorbeeld **5× B-WE-KL-DATA_DUCT-G & 1× B-WE-KL-DATA-G**.
- Klik op de groepskop om de afzonderlijke kabelobjecten te tonen. Klik op een kabel om de bijbehorende kaartmarkering en kabelinformatie te bekijken.
- Klik nogmaals op de groepskop om de kabels weer in te klappen. Een groep met de geselecteerde kabel blijft oranje herkenbaar.
- Kabels op verschillende plaatsen langs de selectielijn blijven apart. De groepering verandert niet door in- of uitzoomen en telt hetzelfde tekenobject maar één keer.
- De bestaande wisselknop blijft tussen de afzonderlijke gevonden kabelobjecten schakelen. Een losse kabel krijgt de vertrouwde compacte regel.

De uitleg in **Info en handleiding** is aangevuld. De verbeterde uitlijning en volledig zichtbare onderste knoppen uit versie 1.0.24 zijn opgenomen.

### Publicatie zonder tests

Op verzoek zijn geen tests, Clippy-controles of applicatietests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
