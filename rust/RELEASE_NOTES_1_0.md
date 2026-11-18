# SleufBase Rust 1.0.33 — puntenwolken tekenen, bewaren en inladen

De puntenwolkeditor sluit aan op de normale SleufBase-werkwijze: begin- en eindpunten bepalen de proefsleuf, kabels en leidingen krijgen dezelfde type- en materiaalkeuzes, en scans blijven beschikbaar in een eigen overzicht met tabel en kaart.

## Nieuw en verbeterd

- **Beginpunt en Eindpunt** zijn afzonderlijke acties in de 3D-editor. De sleufas verschijnt automatisch en de editor toont de horizontale sleuflengte. Opnieuw plaatsen vervangt hetzelfde eindpunt en blijft ongedaan te maken. Deze geometriepunten worden niet als kabelobject geëxporteerd.
- **Type en Materiaal** zijn dropdowns met de keuzes uit de effectieve SleufBase-projectinstellingen, inclusief eigen kabel- en leidingtypen. Een eigen objectnaam, diameter en omschrijving blijven beschikbaar. Onbekende waarden uit bestaande tekeningen blijven behouden.
- **Inladen** gebruikt begin/eind automatisch; de eerdere lege askeuze verdwijnt. Ontbrekende eindpunten worden met een gerichte melding en een bewerkactie aangegeven. Oudere rechte lijnen met twee punten kunnen expliciet als as blijven dienen.
- **Puntenwolken beheren** is een doorzoekbare, sorteerbare tabel met coördinaten, aantallen, sleufstatus en de acties **Bewerken** en **Inladen**. Een overzichtskaart toont de scans met bevestigde RD-coördinaten.
- **Blijvend bewaren:** broninformatie en tekenobjecten worden lokaal buiten het geopende project en buiten de conversiecache bewaard. Een bevestigde editoropslag staat ook in dit overzicht, zodat de scan na opnieuw starten beschikbaar blijft. Projectbestanden bewaren daarnaast hun eigen brongegevens.
- **Opnieuw inladen:** een bestaande sleuf uit dezelfde scan kan worden bijgewerkt of als nieuwe kopie worden toegevoegd. Bewerken wijzigt de bewaarde scan; alleen Inladen neemt de nieuwe brongegevens en sleuf in het project over. Handmatig toegevoegde objecten en aanvullende sleufgegevens blijven bij bijwerken bewaard. Oude kleur- en dieptecorrecties op vernieuwde bronpunten worden verwijderd, zodat het nieuwe type en de gemeten hoogte leidend zijn.
- Bij het inladen bepaal je welke objecten in de doorsnede komen. De volledige tekening blijft daarnaast als bronlaag in het project beschikbaar voor verdere bewerking en export.
- De editor heeft de lichte panelen, oranje acties, typografie en afgeronde bediening van SleufBase, met afzonderlijke onderdelen voor proefsleufgeometrie en kabels/leidingen.
- De correcties voor puntselectie, muisafhandeling, Windows-verbindingen en opstartdiagnose uit 1.0.31 en 1.0.32 blijven behouden.

## Gebruik

1. Open een LAS/LAZ via **Puntenwolk → LAS/LAZ openen** of kies **Bewerken** bij een bewaarde scan.
2. Plaats **Beginpunt** en **Eindpunt** op het maaiveld en teken de kabels/leidingen. Kies hun type en materiaal.
3. Sluit de editor wanneer de opslag is voltooid en kies **Inladen** in het puntenwolkoverzicht.
4. Controleer naam, breedte, objecten en de gewone sleufbewerker. Sla het project op en gebruik de bestaande DXF-sjabloonexport.

## Bestaande gegevens en coördinaten

Puntenwolken uit een ouder Rust-project worden bij het openen aan het overzicht toegevoegd. Open het oorspronkelijke LAS/LAZ opnieuw als er alleen een oude conversiecache zonder opgeslagen project bestaat; die cache bevat onvoldoende broninformatie om zelfstandig een scanrecord te herstellen. De oorspronkelijke bestanden worden niet verplaatst of verwijderd.

De tabel toont het midden van de bronbegrenzing. Alleen bevestigde RD-coördinaten in meters worden op de overzichtskaart en als proefsleuf in het project geplaatst. Andere stelsels blijven bruikbaar in de 3D-editor en directe object-DXF-export. Er wordt geen stelsel of hoogte automatisch omgezet.

Nieuwe projecten gebruiken formaat 7 voor de aanvullende annotatievelden; oudere projecten worden bij het openen gemigreerd. Gebruik deze of een nieuwere SleufBase-versie voor projecten die met 1.0.33 zijn opgeslagen. Neem bij verplaatsing naar een andere computer ook de oorspronkelijke LAS/LAZ mee.

## Controle

Rust-bouwcontrole, JavaScript-syntaxcontrole en Windows-bouw- en pakketverificatie zijn onderdeel van deze release. Geautomatiseerde tests blijven volgens afspraak overgeslagen. Er is geen interactieve Windows-praktijkcontrole uitgevoerd.
