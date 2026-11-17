## SleufBase Rust 1.0.19 voor Windows

### Typekleuren in de virtuele proefsleufeditor

- Kabels en leidingen uit marXact gebruiken nu ook in **Proefsleuf bewerken** de kleuren uit de actieve objectregels.
- Dit geldt voor de doorsnede, het bovenaanzicht, de kleurmarkeringen in de objectenlijst en de geselecteerde kabel of leiding. De oranje selectierand blijft herkenbaar.
- Een expliciet opgeslagen objectkleur heeft voorrang op de typekleur.
- Wanneer je instellingen of objectregels opslaat terwijl de editor openstaat, worden de effectieve editorinstellingen vernieuwd. Projectgebonden kleuren blijven leidend en de lopende conceptbewerking blijft behouden.
- De correctie verandert de weergave; de oorspronkelijke meetcoördinaten en objecteigenschappen blijven behouden.

### Vrije tekst bij Variant in de DXF-sjabloonexport

- **Variant** is nu een blijvend bewerkbaar tekstveld met een keuzepijl voor de bestaande standaardvarianten.
- Je kunt bijvoorbeeld **B1**, **-2** of een eigen omschrijving invullen. Met **Nummer/variant toepassen** neem je de tekst over voor de geselecteerde proefsleuven.
- Eigen varianttekst wordt als achtervoegsel verwerkt en niet meer door de oude metadataherkenning naar een automatische variant omgezet.
- Een leeg veld betekent geen variant. De keuze **Automatisch (B/C/D...)** blijft beschikbaar via de keuzelijst.
- De bestaande controle op geldige labels blijft actief. Een ongeldige combinatie geeft een melding voordat geselecteerde labels worden gewijzigd.

### Openen in de KickTheMap-browser

- Een beschikbare aangemelde browser van hetzelfde account wordt hergebruikt en naar een apart venster verplaatst. De webpagina, browseropslag en aanmelding blijven daarbij in hetzelfde browserproces.
- Een nieuw jobvenster start zonder een gelijktijdig aanmeldverzoek dat de overdracht van de bestaande sessie kan verstoren. Bij een browseraanmelding wordt dezelfde browseridentificatie gebruikt voor vervolgaanvragen.
- Als opnieuw aanmelden nodig is, verschijnt het aanmeldscherm in het aparte browservenster. Na gecontroleerde aanmelding wordt de gekozen job hervat als die nog bij de huidige sessie en het huidige project hoort.
- Een ontbrekende overdraagbare native sessie blokkeert het openen van het aanmeldvenster niet meer.
- Annuleren, uitloggen en een projectwissel stoppen een oude aanvraag, zodat een vertraagd antwoord niet alsnog een eerdere job opent.

### Zoomen in de jobskaart

- Zoomen met het muiswiel houdt het kaartpunt onder de muis op dezelfde plek, ook bij een breed of hoog jobsvenster.
- De minimale en maximale kaartschaal volgen dezelfde grenzen als de hoofdkaart.

### Duidelijker menu Acties

- **Zoom naar alles** heet nu **Zoom naar extent**.
- De groepen in **Acties** zijn van elkaar gescheiden door zichtbare horizontale lijnen.

### Verder inzoomen op percelen

- De percelenachtergrond blijft zichtbaar wanneer je verder inzoomt dan de kleinste schaal waarop PDOK de perceellagen aanlevert.
- SleufBase gebruikt daarvoor de fijnste beschikbare bronweergave en vergroot die naar de gevraagde kaartuitsnede. Verder inzoomen voegt geen nieuwe brongegevens toe.
- Dit geldt ook voor **luchtfoto met percelen** en **OpenStreetMap met percelen**, inclusief perceelgrenzen, labels en bijpijlingen. De onderliggende kaart behoudt zijn eigen detail.
- De RD-uitlijning en de aangevraagde kaartuitsnede blijven behouden; transparante perceellagen worden met behoud van hun transparantie over de basiskaart gelegd.

### Vensters zonder sprong bij het resizen

- **Proefsleuf bewerken** vult vanaf het openen de beschikbare vensterhoogte. De onderste knoppen en meldingen krijgen hun werkelijke ruimte, zodat een eerste resize niet langer uitgaat van een andere hoogte dan het zichtbare venster.
- De editor blijft verplaatsbaar en behoudt zijn onthouden vensterpositie. Alleen bij de eerste opening wordt een beginpositie binnen de schermmarge gekozen.
- Ook de twee DXF-sjabloonexportvensters vullen hun beschikbare hoogte, zodat daar dezelfde afwijking tussen zichtbare en opgeslagen venstermaten wordt voorkomen.

### Publicatie zonder tests

Op verzoek zijn voor deze release geen tests uitgevoerd. De releaseworkflow bouwt de Windows-toepassing, maakt de installer en portable ZIP en controleert de pakketinhoud, versie, bronrevisie en SHA-256-controlesommen. De teststappen en Clippy worden overgeslagen.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
