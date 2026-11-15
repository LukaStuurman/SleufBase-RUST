## SleufBase Rust 1.0.4 voor Windows

### KickTheMap: betrouwbaarder vernieuwen en aanmelden

- Een tijdelijke fout bij het vernieuwen van de joblijst behoudt de actieve sessie, de bestaande jobs en de selectie. Je kunt opnieuw vernieuwen zonder onnodig opnieuw aan te melden.
- Een aantoonbaar verlopen aanmelding krijgt een duidelijke melding om opnieuw in te loggen. Verouderde resultaten kunnen een latere aanmelding niet vervangen.
- De knop **Download annuleren** verschijnt alleen tijdens een download. Andere lopende activiteiten krijgen een passende voortgangsmelding.
- Na een beveiligingscontrolemelding kan gewijzigde accountinvoer weer een gewone aanmeldpoging starten. Een oude melding overschrijft die nieuwe invoer niet.

### Virtuele proefsleuf: behoud van kabels en meetgegevens

- Een later ontvangen AHN-antwoord behoudt de geselecteerde kabel, de volgorde in het open concept en nog niet opgeslagen invoer. Zo kan de hoogteactualisatie geen andere kabel laten overschrijven.
- De definitieve sleuf wordt bij het opslaan nog steeds op afstand gesorteerd.
- Een kabel wegslepen en terugbrengen herstelt nu de exacte oorspronkelijke positie en diepte. Ook de keuze tussen een ingemeten absolute hoogte en relatieve diepte blijft behouden.

### DXF-sjabloonexport: duidelijke fouten zonder verlies van projectinstellingen

- Exportvoorkeuren worden gevalideerd voordat ze in het project worden bewaard en voordat de exporttaak start. Ongeldige tekst kan het latere opslaan van het project daardoor niet blokkeren.
- Fouten noemen het betreffende veld of proefsleufvak en geven aan wat je kunt aanpassen. Het formulier, de projectgegevens en eerder opgeslagen voorkeuren blijven beschikbaar.
- Opmerkingen met Windows-regeleinden en tabs blijven ongewijzigd bruikbaar bij export en bij het opslaan en opnieuw laden van het project.
- Het open exportvenster controleert sjabloon-, uitvoer- en fotopaden niet langer bij ieder schermbeeld. De bestandscontroles blijven bij het expliciet starten van de export plaatsvinden.

### Downloaden

De Windows-installer en portable ZIP hebben ieder een SHA-256-controlebestand. Sluit SleufBase voor het bijwerken. Pak de portable ZIP volledig uit voordat je het programma start.

De Gemma-AI-functie blijft verwijderd. De vernieuwde schermindeling uit versie 1.0.3 blijft beschikbaar.
