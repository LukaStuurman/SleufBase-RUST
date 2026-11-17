## SleufBase Rust 1.0.27 voor Windows

### Opdrachtgeverslogo's beheren en kiezen

- Voeg eigen PNG- of JPEG-logo's toe via **Instellingen → DXF-export → Opdrachtgeverslogo's**. Geef ieder logo een herkenbare naam en een **Afkorting op blad**.
- Kies een standaard opdrachtgeverlogo. De ingebouwde logo's blijven beschikbaar.
- De DXF-sjabloonexport heeft een keuzelijst met de bewaarde logo's. Nieuwe opdrachtgeverslogo's voeg je voortaan in Instellingen toe.
- Bij het kiezen van een opdrachtgever verandert de projectnummerkop op de gebruikte DXF-bladen mee, bijvoorbeeld van **PROJECTNR. BGM:** naar **PROJECTNR. ENEXIS:**. Het ingevulde projectnummer blijft behouden.
- Nieuwe logo's worden pas bij **Opslaan** als eigen kopie bewaard. Annuleren behoudt de bestaande instellingen; oudere eigen logokeuzes blijven beschikbaar.

### Contactgegevens en uitlijning

- Het standaard e-mailadres is **contact@techbasebv.nl**. Het voormalige standaardadres wordt bij het laden van opgeslagen instellingen bijgewerkt; andere ingevulde adressen blijven behouden.
- Het eerste kabelobject begint op dezelfde hoogte als de witte kaartcontainer. Beide panelen gebruiken dezelfde titelafstand.
- De groepering met een afzonderlijke regel per laag uit 1.0.26 blijft behouden.
- **Info en handleiding** is aangevuld met het beheren en kiezen van opdrachtgeverslogo's.

### Publicatie zonder tests

Op verzoek zijn geen tests, Clippy-controles of applicatietests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
