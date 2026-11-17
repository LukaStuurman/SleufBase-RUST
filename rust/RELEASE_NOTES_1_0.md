## SleufBase Rust 1.0.18 voor Windows

### Iedere marXact-import begint met naamkoppelingen

- Na het lezen van een geldig marXact-bestand verschijnt **altijd** het venster **marXact: kabels en leidingen koppelen**. Ook als alle namen al bekend zijn, controleer je de koppelingen vóór de metingen worden toegevoegd.
- Per bronnaam zie je het aantal meetobjecten en kies je een kabel- of leidingtype uit de actieve woordenlijst. Opgeslagen koppelingen staan vooraf ingevuld. Onbekende namen vragen een bewuste keuze; met **Negeren** laat je niet-gewenste meetpunten weg.
- **Deze koppelingen onthouden voor volgende imports** staat standaard aan. De koppelingen blijven beschikbaar na herstart en bij andere projecten. Je kunt onthouden uitschakelen voor alleen de huidige import.
- De oorspronkelijke marXact-laagnamen krijgen de juiste voorstellen, onder andere laag-/middenspanning, gas met lage/hoge druk en beide riooltypen. Deze expliciete koppelingen voorkomen onjuiste herkenning door losse woorddelen.
- **Annuleren**, sluiten en een projectwissel voegen niets toe. Wanneer opslaan van de koppelingen mislukt, blijven je keuzes beschikbaar.
- Het koppelen of negeren verandert geen gemeten sleufgrens, meetas of XYZ-coördinaten van behouden objecten. De originele meetnamen en eigenschappen blijven bij de objecten bewaard.

### Juiste kabelkleuren in de DXF-sjabloonexport

Kabels en leidingen zonder een eigen vastgelegde kleur gebruiken nu de kleuren uit de actieve objectregels in het detailraster, de kaart en het overzicht van de DXF-sjabloonexport. Dit herstelt de vaste blauwe weergave van marXact en oudere virtuele proefsleuven. Eigen objectkleuren en de echte pixels van TIFF-beelden, luchtfoto's en foto's blijven behouden.

### Dubbelklikken op overlays

Dubbelklik in **Overlays** op een jobobjectlaag of DXF/DWG-tekening om naar de grenzen van die laag te zoomen, met dezelfde camerabediening als bij Proefsleuven. Een enkele klik selecteert de laag; de ingestelde zichtbaarheid blijft behouden.

### KickTheMap-jobbrowser en aanmelding

De bestaande aanmelding wordt overgedragen aan de cookieopslag van het concrete jobvenster en daar gecontroleerd vóór de jobpagina opent. Een hergebruikt venster dat terug op de aanmeldpagina staat kan eenmaal worden hersteld vanuit de actuele aanmelding van SleufBase. Een fout in het aparte venster wordt niet meer ten onrechte gemeld als een verlopen aanmelding van het gehele programma. Een nog geopende tekening wordt eerst hergebruikt om conceptinvoer te behouden.

### Jobs-kaart blijft zichtbaar tijdens bewegen

Beschikbare kaartdelen schuiven op hun juiste geografische positie mee tijdens verplaatsen en zoomen. Ontbrekende kaartdelen kunnen ook tijdens het slepen worden opgehaald. Aanvragen worden begrensd en bruikbare lopende downloads mogen afronden. Een kaartvernieuwing of mislukte download wist het beschikbare kaartbeeld niet.

### Controle

De meegeleverde marXact-DXF is lokaal met de Rust-importeur ingelezen: **26 proefsleuven, 211 toegewezen meetobjecten en 15 verschillende bronnamen**. De controle omvat naamkoppelingen, behoud van meetgeometrie en eigenschappen en opslaan/opnieuw lezen van het project. De oorspronkelijke tekening is niet in de repository opgenomen.

Gerichte regressies dekken de koppeldialoog en opslag, semantische exportkleuren, overlayzoom, browseroverdracht en het behouden van het kaartbeeld tijdens bewegen. De Windows-build en publicatieworkflow controleren de installer, portable ZIP, versie, bronrevisie en SHA-256-controlesommen. De KickTheMap-overdracht is gecontroleerd met synthetische browserantwoorden; hiervoor is geen echte accountaanmelding uitgevoerd.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** voor installatie of bijwerken, of pak **SleufBase-Rust-portable.zip** volledig uit. Sluit SleufBase voordat je bijwerkt. Bij beide downloads staat een SHA-256-controlebestand.
