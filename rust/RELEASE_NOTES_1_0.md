## SleufBase Rust 1.0.17 voor Windows

### Invoervelden direct gebruiken

De verborgen KickTheMap-aanmelding neemt bij het starten geen toetsenbordfocus meer over. Een klik in het SleufBase-hoofdvenster herstelt de focus gericht naar dat venster, zodat je in invoervelden kunt typen zonder eerst naar een andere app en terug te schakelen.

### Een apart KickTheMap-jobvenster

- **Open in KickTheMap-browser** opent de geselecteerde job in een eigen, verplaatsbaar en schaalbaar venster. Het hoofdvenster en de Jobs-lijst blijven beschikbaar.
- De kabel- en leidingvelden staan op de KickTheMap-webpagina zelf, met de bediening uit de oorspronkelijke SleufBase: **Kabel/Leiding afkorting**, **Materiaal**, **Diameter**, **Bundel (aantal)** en **Dekband**.
- De keuzelijsten gebruiken de effectieve projectinstellingen, inclusief extra profielkeuzes en materialen. Bestaande afwijkende waarden blijven behouden. Je tekent en slaat het object op met de eigen bediening van KickTheMap.
- Sluiten met het kruisje verbergt het jobvenster. Dezelfde job opnieuw openen brengt het venster terug en behoudt de nog niet opgeslagen invoer.
- **Handmatige GeoTIFF-vastlegging** opent eerst de juiste proefsleufpagina in dit aparte venster. De vastlegging start nadat de pagina gereed is. Via het Jobs-venster kun je de vastlegging annuleren.
- Na opslaan op de website kun je vanuit Jobs de geselecteerde **objecten opnieuw laden** om de lokale joblaag bij te werken.

### Compactere en flexibelere Jobs

- De belangrijkste acties staan bij elkaar, met een duidelijke hoofdknop voor laden en aparte acties voor openen en vernieuwen.
- De jobregels zijn compacter en hebben minder witruimte. Kolommen gebruiken de beschikbare vensterbreedte; aanvullende gegevens blijven via de aanwijzer beschikbaar wanneer een smal venster minder kolommen toont.
- Zoeken, sorteren, meervoudige selectie en het scrollen door grote joblijsten blijven beschikbaar. De indeling past mee bij het vergroten en verkleinen van het venster.

### Selectie en scherpere Jobs-kaart

- De kaart reserveert een sleepbeweging binnen de kaart, zodat het verplaatsbare Jobs-venster de selectie niet meer overneemt.
- Sleep met de **linkermuisknop** een selectievak om meerdere jobpunten te kiezen. **Ctrl** of **Shift** voegt toe aan de selectie; **Esc** annuleert het vak. Midden/rechts slepen verplaatst de kaart en het muiswiel zoomt.
- De kaart haalt bronkaarten met meer detail op, met tweemaal de logische beeldresolutie binnen een begrensd geheugen- en tegelbudget. Verouderde aanvragen worden geannuleerd bij een andere zoomstand of venstergrootte.
- De kaart behoudt de juiste verhoudingen bij het wijzigen van het vensterformaat.

### Build zonder tests

Op uitdrukkelijk verzoek zijn voor deze release **geen tests uitgevoerd**: geen Rust-tests, browser- of formuliercontracttests, Clippy of installatie-/starttests. De broncode is beoordeeld en de Windows-releasebuild, het samenstellen van de installer en portable ZIP en de controles van pakketinhoud, versie, bronrevisie en SHA-256 blijven onderdeel van de publicatie. Deze controles zijn geen bevestiging van de werking op een echte gebruikerscomputer.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start. Bij beide downloads staat een SHA-256-controlebestand.
