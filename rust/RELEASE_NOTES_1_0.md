## SleufBase Rust 1.0.26 voor Windows

### Iedere laag op een eigen regel

De groepskop in **Kabelobjecten** toont iedere laag voortaan op een afzonderlijke regel, met het aantal objecten ervoor. Het &-teken tussen laagnamen is verwijderd. Bijvoorbeeld:

**5× B-WE-KL-DATA_DUCT-G**

**1× B-WE-KL-DATA-G**

- Alle laagregels blijven zichtbaar; de kop past zijn hoogte aan de inhoud aan.
- Klik op de groepskop om de afzonderlijke kabelobjecten uit of in te klappen.
- Individuele kabelselectie, oranje markering en de wisselknop blijven behouden.
- De uitleg in **Info en handleiding** is bijgewerkt.

### Publicatie zonder tests

Op verzoek zijn geen tests, Clippy-controles of applicatietests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
