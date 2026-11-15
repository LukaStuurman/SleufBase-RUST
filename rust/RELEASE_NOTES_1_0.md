## SleufBase Rust 1.0.5 voor Windows

### Virtuele proefsleuf: sneller toevoegen en duidelijker bewerken

- **Toevoegen en herhalen** bewaart de huidige kabel of leiding en zet dezelfde eigenschappen klaar voor de volgende. Kies daarna een nieuwe positie. Bij een bestaande selectie heet de actie **Bewaren en herhalen**.
- Een klaargezette herhaling wordt pas een nieuwe kabel na invoer. Alleen het project opslaan voegt geen ongebruikte herhaling toe. Ongewijzigde meetgegevens worden bij herhalen behouden.
- **Invoer wissen** en **Wijzigingen herstellen** herstellen alleen het actieve formulier. Eerder toegevoegde kabels, conceptwijzigingen en ontvangen AHN-hoogtes blijven bewaard.
- Nummers in de doorsnede en kleurpunten in de objectenlijst maken de koppeling tussen kabel en invoer duidelijker. Klikken op een nummer selecteert dezelfde kabel als klikken op de cirkel.
- Kabelacties en **Opslaan in project** staan op afzonderlijke regels. Lange meldingen hebben een begrensd, scrollbaar vlak zodat de knoppen bereikbaar blijven.
- Typen en materialen volgen de projectinstellingen. Ook materialen uit oudere Python-projectinstellingen worden overgenomen.

### DXF-sjabloonexport: zie vooraf welk vak en label wordt gebruikt

- Iedere sleufkaart toont het echte exportvak en het uiteindelijke label, bijvoorbeeld **Exportvak 1 · PS12B**. Automatische B/C/D-varianten zijn direct zichtbaar.
- Uitgesloten sleuven tonen **Niet in export** en tellen niet mee. Opgenomen lege vakken houden hun plaats in het sjabloon.
- **Regel 01**, **Regel 02**, enzovoort geven de bewerkselectie aan. Uitleg maakt duidelijk hoe je meerdere regels selecteert en dat **Meenemen** bepaalt welke sleuven worden geëxporteerd.
- Een ongeldig label krijgt een herstelhint bij de betreffende kaart. De melding onderaan noemt het regelnummer, zodat je de invoer snel terugvindt.

### KickTheMap: passende accountstatus en uitleg

- Tijdens het vernieuwen van jobs en downloaden blijft de verbonden accountkaart zichtbaar wanneer de eigenaar van de taak bekend is.
- De getoonde identiteit komt uit de bevestigde sessie. Bij een browsersessie vermeldt het venster dat je via de KickTheMap-pagina bent aangemeld.
- De uitleg bij **Inloggegevens onthouden en automatisch aanmelden** volgt de gekozen optie. Sluiten en heropenen schakelt een bewust uitgevinkte keuze niet opnieuw in.
- De wachtwoordhulp legt uit dat een bewaard wachtwoord alleen bij hetzelfde e-mailadres hoort.

### Downloaden

De Windows-installer en portable ZIP hebben ieder een SHA-256-controlebestand. Sluit SleufBase voor het bijwerken. Pak de portable ZIP volledig uit voordat je het programma start.

De Gemma-AI-functie blijft verwijderd. De aanmeldcontroles gebruiken lokale testgegevens; een live aanmelding met een persoonlijk KickTheMap-account is niet uitgevoerd.
