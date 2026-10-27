# SleufBase Rust UI preview 4

Deze Windows-preview verbetert de bediening, de proefsleufeditor, DXF-sjabloonexport en de browseraanmelding.

## Verbeteringen

- Bovenste knoppen en submenu-opties hebben zichtbare randen en duidelijke oranje feedback wanneer de muis erboven staat.
- De virtuele-proefsleufeditor heeft directe navigatie naar sleufgegevens en meetobjecten. Opslaan blijft bereikbaar buiten het scrollgedeelte; wijzigingen worden eerst als concept gecontroleerd.
- Beide DXF-exportvensters passen zich aan het venster aan en houden hun exportknop bereikbaar. Projectnaam en Tekenaar staan ieder op een eigen rij, direct vóór het bijbehorende invoerveld.
- Het ingebouwde DXF-sjabloon is standaard, ook voor virtuele proefsleuven. Een eigen DXF-sjabloon is alleen een optionele, ingeklapte keuze.
- Kaarttegels worden met maximaal vier tegelijk opgehaald, met behoud van cachegebruik, correcte plaatsing en annulering. Dit vermindert het wachten op opeenvolgende netwerkverzoeken.
- KickTheMap-browseraanmelding haalt de sessie en live joblijst eerst op via het eigen ingelogde Edge-tabblad. Aanmeldstatus en reCAPTCHA-instructies zijn zichtbaar in het inlogvenster. De verouderde melding dat browserlogin nog geïntegreerd moet worden is verwijderd.

De kaart-, zoom-, instellingen- en dubbelklikverbeteringen uit preview 3 zijn inbegrepen.

## Downloads

- **SleufBase-Rust-preview-Setup.exe**: aparte Windows-installatie.
- **SleufBase-Rust-portable-preview.zip**: uitpakken en `sleufbase-rust.exe` starten.
- De bijbehorende `.sha256`-bestanden zijn bedoeld om de downloads te controleren.

## Controle en beperkingen

De UI-wijzigingen zijn gecontroleerd met 881 geslaagde Windows-MSVC-tests, waaronder 200 UI-tests en controles op verschillende vensterformaten en schaalinstellingen. De release wordt alleen gepubliceerd na een geslaagde Windows-run op de huidige main, inclusief codecontrole, geoptimaliseerde build, pakket- en installercontrole. Pakketinhoud, revisie en controlesommen worden vóór publicatie opnieuw gecontroleerd.

Dit blijft een preview. **De echte aanmelding met een KickTheMap-account en interactieve reCAPTCHA is nog niet handmatig bevestigd.** Meld je zelf aan in het door de app geopende Edge-venster en kies daarna Sessie overnemen. De code- en sessiecontroles slagen; dit is geen bevestiging van een volledige praktijkaanmelding.

De visuele controles zijn zonder Computer Use uitgevoerd met gerenderde UI-beelden. Native GPU-/AutoCAD-weergave, praktijkprojecten en live snelheid van kaartdiensten zijn niet volledig geverifieerd. De installatiebestanden zijn niet digitaal ondertekend.
