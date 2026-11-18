# Handleiding SleufBase Rust

SleufBase brengt proefsleuven, meetobjecten en kabeltekeningen samen op één kaart. Je kunt gegevens laden, aanvullen en bewaren en daarna kaarten, DXF-tekeningen en overzichten maken.

Deze handleiding volgt de gewone menubalk van het programma. **Acties → Project opslaan** betekent bijvoorbeeld: open het menu **Acties** en kies **Project opslaan**. Woorden als *job*, *overlay* en *variant* worden uitgelegd bij de handeling waarvoor je ze nodig hebt.

## Hulp in het programma

Klik op **Info** of druk op **F1**. Kies een onderwerp of zoek op een functie, bijvoorbeeld *maaiveld*, *variant* of *GeoTIFF*. De uitleg is lokaal beschikbaar, ook zonder internet. Met **Uitleg kopiëren** kopieer je het getoonde onderwerp om het elders te bewaren of te delen. Je kunt het Info-venster verplaatsen en de grootte aanpassen; de onderwerpen en uitleg hebben hun eigen scrollruimte.

Zie [Installeren en bijwerken](PORTABLE.md) als je het programma nog moet installeren of een nieuwere versie wilt gebruiken.

## Inhoud

1. [Snel beginnen](#1-snel-beginnen)
2. [Het hoofdscherm en de kaart](#2-het-hoofdscherm-en-de-kaart)
3. [Proefsleuven en overlays selecteren](#3-proefsleuven-en-overlays-selecteren)
4. [GeoTIFF en KLIC laden](#4-geotiff-en-klic-laden)
5. [Kabelinformatie en lijnselectie](#5-kabelinformatie-en-lijnselectie)
6. [Aanmelden bij KickTheMap](#6-aanmelden-bij-kickthemap)
7. [Jobs zoeken, selecteren en laden](#7-jobs-zoeken-selecteren-en-laden)
8. [De kaart in het Jobs-venster](#8-de-kaart-in-het-jobs-venster)
9. [Kabels en leidingen in de KickTheMap-browser](#9-kabels-en-leidingen-in-de-kickthemap-browser)
10. [Handmatig een GeoTIFF vastleggen](#10-handmatig-een-geotiff-vastleggen)
11. [marXact-metingen importeren](#11-marxact-metingen-importeren)
12. [Virtuele proefsleuven maken en bewerken](#12-virtuele-proefsleuven-maken-en-bewerken)
    [LAS- en LAZ-puntenwolken verwerken](#12a-las--en-laz-puntenwolken-verwerken)
13. [Kabels en leidingen in de doorsnede](#13-kabels-en-leidingen-in-de-doorsnede)
14. [Beginpunten, referenties, maaiveld en opmerkingen](#14-beginpunten-referenties-maaiveld-en-opmerkingen)
15. [DXF-sjabloonexport](#15-dxf-sjabloonexport)
16. [Volgorde, nummering, variant en schaal](#16-volgorde-nummering-variant-en-schaal)
17. [Eigen sjabloon, stempel en logo's](#17-eigen-sjabloon-stempel-en-logos)
18. [Andere exports](#18-andere-exports)
19. [Instellingen, projecten en back-ups](#19-instellingen-projecten-en-back-ups)
20. [Bediening en sneltoetsen](#20-bediening-en-sneltoetsen)
21. [Problemen oplossen](#21-problemen-oplossen)

## 1. Snel beginnen

1. **Laad je gegevens.** Gebruik **KickTheMap → Jobs** voor online metingen, **Acties → Laad GeoTIFF(s)** voor beelden, **Acties → Laad KLIC DXF** voor een kabeltekening of **marXact → marXact import** voor meetbestanden.
2. **Zoek de juiste plek.** Dubbelklik links op een proefsleuf of overlay. Met **Acties → Zoom naar extent** zoom je naar de zichtbare gegevens.
3. **Bekijk en vul aan.** Selecteer kabels voor informatie. Rechtsklik op een proefsleuf voor de beschikbare bewerkingen. Een eigen sleuf maak je met **Virtuele proefsleuf** onder de lijst.
4. **Controleer de sleufgegevens.** Denk aan de ligging, kabeltypen, diameter, hoogte of diepte, beginpunt, referentie en opmerkingen.
5. **Sla het project op.** Gebruik **Acties → Project opslaan** en kies een herkenbare projectnaam en locatie.
6. **Maak de uitvoer.** Kies **Acties → Proefsleuven sjabloon DXF** voor kaarten en profielen in één tekening. Controleer daar welke sleuven daadwerkelijk meegaan.

Je hoeft niet alle bronsoorten te gebruiken. Je kunt bijvoorbeeld beginnen met alleen marXact-metingen of een KLIC met zelf getekende sleuven.

## 2. Het hoofdscherm en de kaart

De menubalk bovenaan bevat **Locatie**, **Acties**, **KickTheMap**, **Instellingen**, **marXact** en **Info**. Links staan **Proefsleuven**, **Overlays** en **Kabelinformatie**. De grote werkkaart staat daarnaast; de lijst **Kabelobjecten** kan rechts van de kaart worden geopend.

| Onderdeel | Waarvoor gebruik je het? |
| --- | --- |
| Proefsleuven | GeoTIFFs, virtuele proefsleuven en marXact-sleuven kiezen, tonen en bewerken. |
| Overlays | Geladen meetobjecten en tekeningen kiezen en tonen. |
| Kabelinformatie | De gegevens van één geselecteerd object lezen, of het aantal objecten en de lagen van een geselecteerde kabelgroep bekijken. |
| Kaart | Navigeren, selecteren en de actieve kaartbewerking uitvoeren. |

### Zoomen en verschuiven

Scroll boven de kaart om te zoomen naar de plek waar de muis staat. Sleep met de middelste muisknop om de kaart te verschuiven. Met de linkermuisknop kan dit ook wanneer er geen teken- of plaatsgereedschap actief is. Bij zo'n gereedschap heeft de linkermuisknop de functie die in de statusmelding wordt aangegeven.

Gebruik **Locatie** of **Ctrl+L** om een adres, plaats of RD-/GPS-coördinaat te zoeken. Klik nogmaals op **Locatie** om het zoekpaneel te sluiten. **Ctrl+L** opent het paneel en zet de cursor in het zoekveld, ook als het al openstaat. Met de pijltjestoetsen kies je een zoeksuggestie en met Enter ga je ernaartoe.

### Achtergrond en namen

Kies de achtergrond in de keuzelijst **Kaart** direct boven het kaartbeeld. Beschikbaar zijn onder meer PDOK-luchtfoto, Kadaster, OpenStreetMap en combinaties met percelen. Perceelgrenzen en -nummers komen van Kadaster/PDOK; de zichtbare details zijn afhankelijk van de bron en het zoomniveau.

Met **Proefsleufnaam**, links van **Maaiveld invullen**, toon of verberg je de naambordjes. Dezelfde voorkeur staat onder **Instellingen → Kaarten → Kaartscherm**. Als er onvoldoende ruimte is zonder overlap, kan een naambordje tijdelijk ontbreken. Zoom of verschuif de kaart om de omgeving duidelijker te maken.

De zichtbaarheid van een TIFF regel je met het vinkje bij de proefsleuf. De keuzes op de werkkaart en de instellingen voor een export zijn afzonderlijke keuzes.

## 3. Proefsleuven en overlays selecteren

Klik op een regel om die te selecteren. **Dubbelklik** op een proefsleuf of overlay om naar de ligging ervan te zoomen. Rechtsklikken opent het menu met de bewerkingen die voor die bron beschikbaar zijn.

In deze twee lijsten werkt meervoudig selecteren zo:

| Handeling | Resultaat |
| --- | --- |
| Gewoon klikken | Eén regel selecteren. |
| Ctrl+klik | Een losse regel aan de selectie toevoegen of eruit halen. |
| Shift+klik | Een aaneengesloten reeks selecteren. |
| Ctrl+Shift+klik | Een reeks aan de bestaande selectie toevoegen. |

Een vinkje bepaalt of de bron op de kaart zichtbaar is. Selecteren bepaalt op welke bron je een handeling uitvoert. Een verborgen bron blijft in het project aanwezig.

Gebruik **Verwijder selectie** om gekozen regels uit het project te halen. Dit kan ook via het contextmenu of met Delete/Backspace wanneer de betreffende lijst actief is en er geen bewerkvenster of popup voor staat. De oorspronkelijke bronbestanden op schijf worden hierdoor niet verwijderd.

Met **Jobpunten** onder Overlays zet je de jobpunten gezamenlijk aan of uit. Tekeningen houden hun eigen zichtbaarheid.

## 4. GeoTIFF en KLIC laden

### GeoTIFF

Kies **Acties → Laad GeoTIFF(s)** en selecteer een of meer `.tif`- of `.tiff`-bestanden. SleufBase leest hun geografische ligging. Bij een KickTheMap-download kan een bijbehorend `.job.json`-bestand staan; laat dit naast het gedownloade bestand staan zodat de jobidentiteit beschikbaar blijft.

De beelden verschijnen in **Proefsleuven**. Zet het vinkje aan en dubbelklik op een beeld om het terug te vinden. **Acties → Toon TIFF-adressen** opent het adressenoverzicht. Rechtsklik op een TIFF voor onder meer rotatie, een opmerking, een virtuele kopie en het kopiëren van het midden.

Het TIFF-adressenvenster kun je vergroten en verkleinen. De lijst gebruikt de beschikbare hoogte en de kolommen passen zich aan de breedte aan. In een smal venster staan bestandsnaam en adres onder elkaar. De kopieerknoppen en **Vernieuwen** blijven onderaan beschikbaar en schuiven zo nodig naar een volgende regel. Selecteer een adresregel om die te kopiëren; dubbelklikken kopieert direct het adres.

### KLIC-DXF

Kies **Acties → Laad KLIC DXF** en selecteer een DXF. De tekening verschijnt onder **Overlays**. Toon de gewenste tekening en zoom in om dicht bij elkaar liggende kabels te kunnen onderscheiden.

Bij een groot overzicht vereenvoudigt de werkkaart zeer kleine details. Tekst en fijne geometrie worden duidelijker als je inzoomt. Die overzichtsweergave verandert de oorspronkelijke objectgegevens niet.

## 5. Kabelinformatie en lijnselectie

### Eén kabel of overlappende kabels

Klik op een kabel in de kaart. De geselecteerde kabel krijgt een gloed en de gegevens verschijnen onder **Kabelinformatie**. Je ziet daar onder meer de beschikbare XDATA InfraCAD, laagnaam, bronbestand en objectgegevens.

Als er meer kabels op de geklikte plek liggen, gebruik je de wisselknop naast **Kabelinformatie** om tussen de gevonden objecten te wisselen. **Lijst (aantal)** opent rechts de gevonden kabelobjecten. Klik op een regel om die kabel te bekijken; de oranje selectie, de kaartmarkering en de informatie horen bij hetzelfde object.

Klik nogmaals op **Lijst** om het rechterpaneel te sluiten. De compacte regels tonen de laagnaam en de beschikbare kabelinformatie. Een volgende kaartklik vult de kandidaten opnieuw. Klik naast de kabels om de kabelselectie op te heffen.

### Kabels vinden met een lijn

1. Klik op het lijnicoon rechts van **Lijst**.
2. Klik in de kaart op het beginpunt van de selectielijn.
3. Klik op het eindpunt.
4. Bekijk rechts de zichtbare lijnobjecten die de lijn raken of kruisen.
5. Klik op een groepsnaam om alle kabels in die groep te selecteren. Gebruik het pijltje om de groep open te klappen en kies een kabelregel als je één kabel wilt bekijken. De wisselknop kiest telkens één gevonden kabel.

Tijdens het tekenen zie je een voorbeeldlijn. Escape, rechtsklikken of opnieuw op de lijnknop klikken annuleert de handeling; de vorige kandidatenlijst blijft dan behouden. De selectielijn dient om objecten te vinden en wordt geen nieuwe kabel in je project.

### Groepen in Kabelobjecten

Na een lijnselectie staan kabels die de selectielijn op vrijwel dezelfde plek raken of kruisen samen in een groep. Daarbij kunnen kabels van verschillende lagen in dezelfde groep staan. Elke groep begint ingeklapt. In de groepskop krijgt elke laag een eigen regel, met eerst het aantal objecten en daarna de laagnaam, bijvoorbeeld:

```text
5× B-WE-KL-DATA_DUCT-G
1× B-WE-KL-DATA-G
```

Alle lagen blijven zichtbaar; de groepskop groeit mee met het aantal regels. Staat er maar één object op die plek, dan blijft het een losse regel.

Klik op de **groepsnaam of het groepsvak** om alle kabelobjecten in de groep te selecteren. De geselecteerde groep en de geselecteerde kabelregels worden oranje. Alle geselecteerde kabels krijgen tegelijkertijd een markering op de kaart. Onder **Kabelinformatie** zie je het totale aantal geselecteerde objecten en de aantallen per laag.

Klik alleen op het **pijltje** om de losse kabelobjecten te tonen of weer te verbergen. Uit- en inklappen verandert de selectie niet. Je kunt een groep dus ingeklapt laten terwijl alle kabels daarin geselecteerd blijven.

Klik op een afzonderlijk kabelkaartje om alleen die kabel te selecteren. De kaartmarkering en **Kabelinformatie** horen dan weer bij dat ene object. De wisselknop blijft tussen de afzonderlijke gevonden kabels wisselen.

In- of uitzoomen verandert de groepen niet. Een object dat de selectielijn meermaals raakt of kruist, staat één keer in de lijst, bij het eerste raak- of kruispunt vanaf het beginpunt.

## 6. Aanmelden bij KickTheMap

Open **KickTheMap → Inloggen** en vul je e-mailadres en wachtwoord in. Met **Inloggegevens onthouden en automatisch aanmelden** worden de gegevens na een geslaagde aanmelding versleuteld bewaard voor je Windows-gebruikersaccount. SleufBase probeert dan bij de volgende start automatisch aan te melden.

Zet je onthouden uit, dan geldt de volgende geslaagde aanmelding alleen voor de huidige sessie. Eerder bewaarde accountgegevens worden daarbij verwijderd. **KickTheMap → Uitloggen** verwijdert de bewaarde aanmelding en schakelt automatisch aanmelden uit.

Soms vraagt KickTheMap om een extra beveiligingscontrole. Rond die af in de websiteweergave binnen SleufBase. Onder **Andere manier van aanmelden** kun je **Aanmelden via KickTheMap-pagina** gebruiken wanneer de gewone aanmelding niet voldoende is.

Het accountvenster toont of je verbonden bent. Een tijdelijke fout bij het vernieuwen van de joblijst betekent niet automatisch dat je opnieuw moet inloggen: lees eerst de statusmelding en probeer **Vernieuwen** opnieuw.

## 7. Jobs zoeken, selecteren en laden

Een *job* is een meting of proefsleuf in KickTheMap. Open **KickTheMap → Jobs** om de lijst te bekijken. Het venster kan groter of kleiner worden gemaakt; de kolommen en knoppen passen zich aan de beschikbare ruimte aan.

### Zoeken en selecteren

Gebruik de filtervelden bij de kolommen of **Filter / selectie**. Als filters actief zijn, toont de knop het aantal filters. Je kunt onder meer filteren op **Geladen jobs**, **Alleen geselecteerd** en **Downloadbaar**. **Alle filters wissen** toont de volledige beschikbare lijst weer. Klik op een kolomkop om te sorteren; de sorteerkeuze staat ook in het filtermenu.

Een gewone klik op een jobregel kiest één job. Gebruik de vinkjes om losse jobs toe te voegen of uit de selectie te halen. In de joblijst voegt **Ctrl+klik of Shift+klik een reeks vanaf het selectiebegin toe**. Dit verschilt van Ctrl+klik in de lijsten Proefsleuven en Overlays van het hoofdscherm.

### Bestanden laden

| Knop of handeling | Resultaat |
| --- | --- |
| Laad geselecteerde jobs | Objectpunten én GeoTIFFs ophalen en in het project laden. In een smal venster: **Laad selectie**. |
| Meer → Alleen objectpunten laden | Alleen de meetobjecten laden; ze komen onder Overlays. |
| Meer → Alleen GeoTIFF laden | Alleen de beelden laden; ze komen onder Proefsleuven. |
| Dubbelklik op een regel in de lijst | Objectpunten en GeoTIFF van die job laden. |
| Open in KickTheMap-browser | Eén geselecteerde job in een apart venster openen. In een smal venster: **Open browser**. |

**Vernieuwen** haalt de joblijst opnieuw op. Tijdens een lopende taak kunnen knoppen tijdelijk uitgeschakeld zijn. De status onderaan vermeldt de voortgang; **Download annuleren** stopt een lopende downloadtaak. Eventuele fouten staan bij **Downloadfouten**.

Bij **Offline cache** zie je eerder bewaarde jobinformatie. Nieuwe downloads, het jobvenster en het verversen van objecten vereisen een actieve aanmelding en een werkende verbinding.

## 8. De kaart in het Jobs-venster

Kies **Kaart** bovenaan het Jobs-venster. De kaart toont de jobs die aan de actieve filters voldoen en waarvoor een locatie beschikbaar is. **Lijst** brengt je terug naar de tabel.

| Bediening | Resultaat |
| --- | --- |
| Klik op een punt | Die job selecteren. |
| Met links een vak slepen | Jobs binnen het vak selecteren. |
| Ctrl of Shift tijdens selecteren | Gevonden jobs aan de bestaande selectie toevoegen. |
| Muiswiel | Zoomen rond de plek van de muis. |
| Midden of rechts slepen | Kaart verschuiven. |
| Escape | Het actieve selectievak annuleren. |
| Passend | De getoonde jobs in beeld brengen. |
| Zoom naar selectie | Naar de geselecteerde jobs zoomen. |
| Dubbelklik op een punt | Die job in de KickTheMap-browser openen. |

De linkermuisknop is hier voor de selectie. Gebruik dus midden of rechts om te verschuiven. Na een kaartselectie gebruik je onderaan dezelfde laadknoppen als in de lijst. Het verschil bij dubbelklikken is bewust: **in de lijst laden, in de jobskaart het jobvenster openen**.

## 9. Kabels en leidingen in de KickTheMap-browser

1. Meld je aan en open **KickTheMap → Jobs**.
2. Selecteer precies één job.
3. Kies **Open in KickTheMap-browser** of, in een smal venster, **Open browser**.
4. Teken of selecteer een object op de KickTheMap-pagina en open de eigenschappen daarvan.
5. Vul de kabel- of leidinggegevens in de aangepaste velden op de pagina in.
6. Sla het object op met de opslagfunctie van KickTheMap.
7. Ga terug naar Jobs en kies **Objecten opnieuw laden** om de lokale gegevens in SleufBase bij te werken.

Het jobvenster staat los van het hoofdvenster. De invoer is onderdeel van de webpagina; je hoeft geen afzonderlijk formulier naast de pagina in te vullen.

| Veld | Wat vul je in? |
| --- | --- |
| Kabel/Leiding afkorting | Het kabel- of leidingtype uit de projectinstellingen. |
| Materiaal | Het bekende materiaal uit de ingestelde materiaalkeuzes. |
| Diameter | De diameter of bijbehorende diametertekst. |
| Bundel (aantal) | Het aantal bij elkaar horende kabels of leidingen. |
| Dekband | Aangeven dat het object een dekband is; het aantalveld is dan niet actief. |

Type en materiaal zijn keuzelijsten. Een onbekende waarde die al in een object staat, blijft zichtbaar. Wil je nieuwe keuzes aanbieden, pas die aan onder **Instellingen → KickTheMap**, sla de instellingen op en open de job opnieuw.

Het wijzigen van een veld slaat het object niet vanzelf op bij KickTheMap. Doe dat eerst op de website. **Objecten opnieuw laden** staat in een breed Jobs-venster direct bij de acties en in een smaller venster onder **Meer**. Deze functie vervangt de bijbehorende lokale objectlaag door de actuele jobgegevens.

Het kruisje van het aparte jobvenster verbergt het venster. Zolang de pagina en aanmelding beschikbaar blijven, kun je dezelfde job opnieuw openen om verder te gaan. Afsluiten van SleufBase sluit ook het jobvenster.

## 10. Handmatig een GeoTIFF vastleggen

Gebruik deze mogelijkheid wanneer de normale GeoTIFF-download voor een job niet werkt, maar je de download wel op de KickTheMap-pagina kunt starten.

1. Open Jobs en selecteer **één** job terwijl je verbonden bent.
2. Kies **Meer → Handmatige GeoTIFF vastleggen**.
3. Wacht tot de geselecteerde job in het aparte jobvenster is geopend en de status meldt dat de vastlegging klaarstaat.
4. Klik pas daarna op de GeoTIFF-download van die job in KickTheMap.
5. Wacht op de melding in SleufBase dat het bestand is opgehaald en geladen.

De vastlegging wacht maximaal twee minuten op het downloadverzoek. Is de wachttijd verstreken, start de vastlegging dan opnieuw voordat je nogmaals op downloaden klikt. Met **Vastlegging annuleren** in Jobs stop je het wachten.

Opent het jobvenster met een aanmeldscherm, rond die aanmelding eerst af. Gebruik de vastlegging voor de geselecteerde job en wacht op de gereedmelding voordat je de website-download start.

## 11. marXact-metingen importeren

Kies **marXact → marXact import** en selecteer de DXF-meetbestanden. Wanneer er proefsleuven uit de meting zijn gelezen, verschijnt altijd **marXact: kabels en leidingen koppelen** voordat de import wordt overgenomen. Dit gebeurt ook als alle namen al bekend zijn of de meting alleen sleufgrenzen bevat.

1. Bekijk links de oorspronkelijke namen en aantallen.
2. Kies per naam het bekende kabel- of leidingtype.
3. Kies **Negeren** voor meetobjecten die niet als kabel of leiding moeten worden meegenomen.
4. Laat **Deze koppelingen onthouden voor volgende imports** aan als je de keuze wilt hergebruiken.
5. Klik op **Importeren**.

Eerder bewaarde koppelingen zijn al ingevuld, zodat je ze kunt nakijken en aanpassen. De knop Importeren wordt pas bruikbaar als alle gevonden namen een keuze hebben. **Annuleren** neemt de nieuwe metingen niet over. De opgeslagen koppelingen beheer je ook bij **Instellingen → KickTheMap**.

De gekozen typen bepalen de bijbehorende kleuren in de editor en uitvoer. Een expliciet ingestelde objectkleur heeft voorrang op de kleur uit de type-instellingen.

Via **marXact → Oriëntatie aanpassen** controleer je de richting van een sleuf. Met de rotatiecorrectie draai je de uitlijning; **Automatisch opnieuw bepalen** berekent die opnieuw uit de meting. In het sleufbewerkvenster blijven de oorspronkelijke 3D-meetgrenzen beschermd. Gebruik **Handmatige kopie** wanneer je een afzonderlijke bewerkbare versie met een andere as nodig hebt.

## 12. Virtuele proefsleuven maken en bewerken

Klik onder **Proefsleuven** op **Virtuele proefsleuf**. Klik in de kaart eerst het beginpunt en daarna het eindpunt. Houd minimaal 0,5 meter tussen beide punten. Escape of rechtsklikken annuleert het tekenen.

Open het bewerkvenster via het contextmenu van de sleuf. Het venster heeft twee onderdelen:

- **Doorsnede:** kabels en leidingen plaatsen en bewerken in een visuele weergave.
- **Sleufgegevens:** naam, breedte, geometrie, hoogtes, dekbanden en aanvullende gegevens.

Je kunt het venster aan de titelbalk verplaatsen en via de rand vergroten of verkleinen. De inhoud blijft scrollbaar; de acties onderaan horen bij het hele concept.

In **Sleufgegevens** kun je bij een handmatige sleuf begin- en eindcoördinaten wijzigen, begin en einde omkeren en de hele sleuf verschuiven. Via het contextmenu op de kaart zijn ook verplaatsen, interactief draaien, een hoek invullen en een virtuele kopie beschikbaar.

Het bewerkvenster werkt met een **concept**. **Opslaan in project** neemt de sleuf en de invoer over in het geopende project. **Sluiten zonder opslaan** verwerpt het concept. Sla daarna met **Acties → Project opslaan** ook het projectbestand op schijf op.

## 12a. LAS- en LAZ-puntenwolken verwerken

Met **Puntenwolk → LAS/LAZ openen** verwerk je scans van KickTheMap of andere leveranciers. Een online account is hiervoor niet nodig. Je kunt LAS/LAZ ook naar het hoofdvenster slepen.

1. Kies het bestand en geef de scan een herkenbare naam. Kies **Onbekend / uit bestand lezen** om beschikbare coördinateninformatie te gebruiken. Bevestig **RD New (EPSG:28992), meters** alleen wanneer de bron daadwerkelijk in RD staat. Een andere keuze verplaatst of herprojecteert de scan niet.
2. Laat SleufBase de puntenwolk voorbereiden. Dit gebeurt lokaal. Bij grote bestanden kan dit even duren; je kunt de import annuleren. Het originele bestand blijft de bron.
3. Plaats **Beginpunt** en **Eindpunt** op het maaiveld aan weerszijden van de proefsleuf. De editor toont automatisch de sleufas en de horizontale lengte. Opnieuw plaatsen vervangt het bestaande begin- of eindpunt; je kunt deze wijziging ongedaan maken.
4. Gebruik **Punt** of **Lijn** voor kabels en leidingen. Klik op zichtbare scanpunten. Sluit een lijn met rechts klikken af. Je kunt objecten selecteren, verplaatsen, verwijderen en wijzigingen ongedaan maken of herstellen.
5. Kies per object het **Type** en **Materiaal** met de dropdowns. De keuzes komen uit dezelfde instellingen als de gewone sleufbewerker, inclusief eventuele projectinstellingen. Vul desgewenst een eigen naam, diameter en bundel/omschrijving in.
6. Sluit de editor nadat de opslagstatus aangeeft dat de wijzigingen zijn verwerkt. De scan en zijn tekenobjecten blijven beschikbaar in het puntenwolkoverzicht, ook na opnieuw starten of openen van een ander project.

**Een proefsleuf inladen.** Open **Puntenwolk → Puntenwolken beheren** en klik bij de scan op **Inladen**. Het begin- en eindpunt bepalen automatisch de sleufas; je hoeft geen aparte as te kiezen. Hun hoogtes vormen de basis voor het maaiveldprofiel. Kies de naam, breedte en kabelobjecten die bij de sleuf horen. Ontbreekt een eindpunt, dan vermeldt het venster wat nog nodig is en kun je de editor openen. Bij oudere tekeningen kan een bestaande rechte lijn met precies twee punten nog expliciet als as worden gebruikt.

Punten binnen de sleuf worden overgenomen. Van kabelpolylijnen worden de echte kruisingen met de sleufas gebruikt, inclusief geïnterpoleerde hoogte en schuinte. De vinkjes bij **Objecten in de doorsnede** bepalen welke objecten het profiel gebruikt. De volledige tekening wordt daarnaast als bronlaag in het project bewaard. Het venster toont hoeveel objecten worden meegenomen of overgeslagen. Lijnen die vrijwel evenwijdig aan de sleufas lopen moet je buiten het dwarsprofiel laten. Begin- en eindpunten worden nooit als kabelobject geëxporteerd. Als deze scan al als sleuf in het project staat, kies je tussen bijwerken en een nieuwe kopie.

Controleer de nieuwe sleuf in de gewone sleufbewerker. Vul ontbrekende gegevens aan en gebruik daarna **Acties → Proefsleuven sjabloon DXF**. De volledige getekende lijnen blijven daarnaast beschikbaar in de puntenwolk en object-DXF-export.

**Overzicht met tabel en kaart.** Puntenwolken beheren toont de bewaarde scans in een doorzoekbare tabel met brongegevens, aantal scanpunten en tekenobjecten, begin/eind-status en coördinaten. **Bewerken** opent de 3D-editor; **Inladen** neemt de gekozen sleuf in het huidige project op. Selecteer een rij om de scan op de overzichtskaart te bekijken. Alleen bronnen met bevestigde RD-coördinaten worden op deze kaart geplaatst. De coördinaten in de tabel zijn het midden van de bronbegrenzing; het zijn geen apart ingemeten referentiepunten.

Het overzicht wordt apart van het geopende project bewaard. **Bewerken** past de opgeslagen scan aan; pas **Inladen** neemt de nieuwe tekening en sleuf over in het huidige project. **Acties → Project opslaan** blijft nodig om ingeladen sleuven en overige projectwijzigingen in je projectbestand te bewaren. Scans uit een ouder Rust-project worden bij het openen van dat project aan het overzicht toegevoegd. Heb je alleen een oude lokale conversie zonder opgeslagen project, kies dan de oorspronkelijke LAS/LAZ opnieuw.

**Andere coördinatenstelsels.** Je kunt scans met lokale of andere coördinaten gewoon bekijken en daarin tekenen. **Objecten naar DXF** bewaart de originele XYZ. Voor plaatsing op de RD-kaart en het maken van een proefsleuf moeten de gegevens RD in meters zijn. Er wordt geen automatische herprojectie of hoogtecorrectie uitgevoerd.

**Verder werken of verplaatsen.** Open de editor opnieuw via Puntenwolken beheren. Neem bij verplaatsing naar een andere computer ook de originele LAS/LAZ mee. Gebruik **Bronbestand opnieuw kiezen** en **Weergave opnieuw opbouwen** als de bron of lokale weergave ontbreekt. SleufBase controleert of het opnieuw gekozen bestand dezelfde scan is; bestaande tekenobjecten blijven bewaard.

## 13. Kabels en leidingen in de doorsnede

### Een object toevoegen

1. Open de sleuf en kies **Doorsnede**.
2. Klik op de gewenste positie in de visuele doorsnede, of gebruik **Kies punt op de kaart**.
3. Kies het **Type** en vul **Materiaal**, **Diameter**, **Aantal** en **Kruishoek** in voor zover bekend.
4. Vul de diepte of hoogte in wanneer je die hebt.
5. Klik op **Kabel/leiding toevoegen**.
6. Herhaal dit voor de overige objecten en kies **Opslaan in project**.

De afstand kan vanaf het begin- of eindpunt worden ingevoerd. De *kettingmaat* is de afstand langs de sleuf vanaf het beginpunt. De nummering in de lijst helpt je het bijbehorende object in de doorsnede terug te vinden.

### Eenheden en betekenis

| Gegeven | Betekenis |
| --- | --- |
| Diameter | In millimeters. De invoer bepaalt ook de weergegeven objectgrootte. |
| Aantal | Het aantal kabels of leidingen in het object of de bundel. |
| Kruishoek | De hoek waarmee het object de sleuf kruist; ook aan te passen in het kleine bovenaanzicht. |
| Diepte invullen | Diepte in meters onder het beginmaaiveld. |
| Z-hoogte handmatig | Absolute hoogte in het gebruikte hoogtestelsel, dus geen diepte onder maaiveld. |
| Dwarsafstand | Zijdelingse afstand ten opzichte van de sleufas. |

Een diepte en een Z-hoogte zijn verschillende gegevens. Voor het afleiden van een absolute objecthoogte uit een diepte is een beginmaaiveldhoogte nodig. Een maaiveldbeschrijving zoals gras of bestrating is weer een ander soort gegeven.

### Herhalen en wijzigen

**Toevoegen en herhalen** bewaart de huidige invoer en neemt type, materiaal, diameter, aantal en kruishoek over voor een volgend object. Kies daarna een nieuwe positie. Selecteer een bestaand object om het te wijzigen; met **Nieuwe kabel/leiding** begin je een nieuw object.

De kleuren volgen de type-instellingen, tenzij het object een eigen kleur heeft. Onder de aanvullende objectgegevens staan onder meer naam, bron/typecode, rol, dwarsafstand en extra attributen.

Dekbanden beheer je via **Dekbanden beheren** in het contextmenu of via **Sleufgegevens**. Vul het segment langs de sleuf, de omschrijving en de begin- en einddiepte in. Voor meerdere meetobjecten kun je onder Sleufgegevens Excel-regels plakken of de objecten naar Excel kopiëren; volg de kolomvolgorde die het venster aangeeft.

## 14. Beginpunten, referenties, maaiveld en opmerkingen

### Beginpunt en referentiepunt

Selecteer de juiste sleuf en gebruik **Beginpunt** of **Referentiepunt** boven de kaart. Volg de statusmelding voor het plaatsen of kiezen van het punt. Bij GeoTIFFs kunnen gemeten beginpunten worden voorbereid vanuit de gekoppelde jobgegevens.

**Acties → Laad alle beginpunten** haalt de beschikbare beginpunten alvast op. Tijdens het kiezen van een beginpunt kun je een andere GeoTIFF in de lijst of kaart selecteren; de punten van die sleuf worden dan voorbereid. **Wis alle referentiepunten** verwijdert de opgeslagen referenties uit het project.

### Maaiveldbeschrijvingen en hoogtes

Met **Maaiveld invullen** boven de kaart kies je de bewerkpunten voor de maaiveldbeschrijving van het profiel. De instellingen hiervoor staan onder **Instellingen → Proefsleuven**. De omschrijving en lijnkleur kun je handmatig aanpassen; BGT kan beschikbare ondergrondinformatie aanvullen.

**Acties → Laad alle maaiveldgegevens** bereidt de bewerkpunten voor de bruikbare sleuven vooraf voor. Je ziet de voortgang en kunt annuleren. Deze voorbereiding behoudt bestaande teksten, kleuren en gemeten hoogten.

De modus Maaiveld invullen blijft actief wanneer je een andere proefsleuf selecteert. Sla een open tekstinvoer eerst op of sluit die voordat je wisselt. Zo hoef je de functie niet voor elke volgende sleuf opnieuw aan te zetten.

AHN-aanvulling in het sleufbewerkvenster gaat over hoogtes. Een beschrijving van het oppervlak, een diepte onder maaiveld en een absolute Z-hoogte zijn niet onderling uitwisselbaar. Bekijk daarom steeds de naam en eenheid van het invoerveld.

### Opmerkingen en coördinaten

Rechtsklik op de sleuf en kies **Opmerking bewerken**. **Opmerking wissen** verwijdert de opmerking van die sleuf. **Acties → Wis opmerkingen** werkt voor het hele project.

Met **Coördinaten midden kopiëren** kopieer je het midden van de sleuf. Het klembord bevat twee RD-getallen met een komma ertussen, zonder X- of Y-label, bijvoorbeeld `123456.000, 456789.000`.

## 15. DXF-sjabloonexport

Open **Acties → Proefsleuven sjabloon DXF**. KickTheMap-, GeoTIFF-, marXact- en virtuele sleuven kunnen samen in de export worden opgenomen. Het venster kan worden verplaatst en vergroot of verkleind.

### Project en uitvoerbestand

Vul **Projectnaam** en **Tekenaar** in. Open zo nodig **Tekeningstempel** voor de overige bladgegevens. Kies onderaan **Uitvoerbestand kiezen…** en geef een nieuwe DXF-bestandsnaam op. De exportdatum wordt automatisch op de gebruikte bladen ingevuld.

### Kaarten en profielen

Kies bij **Kaartgegevens** tussen:

- **Online:** de gekozen kaartachtergrond en beschikbare kaartgegevens ophalen.
- **Offline:** een witte kaart met al geladen gegevens gebruiken; er wordt geen luchtfoto opgehaald.

De achtergrond en kwaliteit van de export staan in dit venster. **Kaartkwaliteit** biedt onder meer 150 en 300 DPI. Bij **Meer exportinstellingen** kun je de kaartresolutie, kaartschaal, BGT-ondergrond, perceelgrenzen, zichtbare CAD-lagen, een eigen sjabloon, logo's en extra CAD-lettertypen instellen.

De kaartresolutie en kaartschaal horen bij de locatiekaart. De **profielschaal** is een afzonderlijke instelling. Een hogere DPI geeft meer beeldpunten; een andere schaal verandert het afgebeelde gebied of de verhouding van het profiel.

### Bronnen en sleuven controleren

Onder **Proefsleuven kiezen** bepaalt **Meenemen** welke sleuven worden geëxporteerd. De oranje regelselectie is bedoeld voor bewerkingen aan volgorde, nummering en schaal. Een geselecteerde regel met Meenemen uit komt dus niet in de tekening.

Controleer per sleuf het uiteindelijke label en exportvak. Bij **Details: bron, schaal en werkfoto's** kun je onder meer de GeoTIFF-koppeling, profielschaal, opmerkingen en werkfoto's bekijken. SleufBase stelt een TIFF voor op basis van jobidentiteit of ligging. Kies zelf de juiste bron als de koppeling ontbreekt of meerdere beelden passen. Voor een virtuele sleuf kan **Eigen virtueel raster** worden gebruikt zonder externe TIFF.

Als een meegenomen vak een noodzakelijke GeoTIFF mist, koppel je de bron of zet je Meenemen uit. Alleen het kiezen van een regel lost een ontbrekende bron niet op.

### Beide kijkrichtingen

Met **Beide kijkrichtingen** maakt SleufBase de normale en omgekeerde richting. In AutoCAD kies je daarna in de blokkeuze **Versie** tussen **Normaal** en **Reverse**. Dit staat los van de vrije Variant bij de PS-nummering en van de versie in het tekeningstempel.

### Export starten en delen

Lees de samenvatting onderaan en klik op **Export starten**. Het voortgangsvenster toont de huidige stap en biedt annuleren zolang dat nog mogelijk is. Wacht op de afrondingsmelding en bekijk eventuele meldingen over ontbrekende gegevens.

Bewaar de DXF en de bijbehorende afbeeldingsbestanden samen. Verplaats of deel de complete export met behoud van de mappenstructuur, zodat CAD de gekoppelde rasters kan vinden. Controleer de uiteindelijke tekening in je CAD-programma voordat je die oplevert.

## 16. Volgorde, nummering, variant en schaal

De gereedschappen rechts van de sleufregels hebben hun eigen scrollvak. De verplaatsknoppen staan bovenaan. Selecteer eerst de regels die je wilt wijzigen.

### Volgorde en lege vakken

Gebruik **Omhoog** en **Omlaag** om de geselecteerde regels te verplaatsen. Je kunt ook een sortering kiezen en **Opnieuw sorteren** gebruiken. Met **Leeg vak invoegen** reserveer je een plek op een exportblad. **Lege vakken verwijderen** haalt geselecteerde lege vakken weg. Een uitgesloten sleuf reserveert geen vak; een bewust leeg vak behoudt zijn plaats.

**GeoTIFF als vak toevoegen…** voegt een geladen TIFF toe die nog niet aan een bestaand vak is gekoppeld. **Alles meenemen** en **Alles uitsluiten** passen de opname van alle sleuven aan. De schakelaars voor dwarsprofielen werken op de meegenomen sleuven.

### PS-nummer en vrije variant

1. Selecteer de regels en zet bij de gewenste sleuven **Meenemen** aan.
2. Vul bij **PS-nummer** het nummer in, bijvoorbeeld `2`.
3. Typ bij **Variant** een korte eigen toevoeging, bijvoorbeeld `B1` of `-2`, of kies via de pijl een standaardvariant.
4. Klik op **Nummer/variant toepassen**.
5. Controleer het uiteindelijke label bij de sleufregels.

Een lege variant betekent geen toevoeging. `AUTO` is gereserveerd voor de automatische varianten; de keuzelijst noemt dit **Automatisch (B/C/D...)**. Begin eigen varianttekst met een letter of teken, zoals `B1` of `-2`. Een los cijfer kan als deel van het PS-nummer worden gelezen en wordt daarom niet als ondubbelzinnige variant geaccepteerd.

**Nummer/variant toepassen** werkt op de geselecteerde, meegenomen sleuven en niet op lege vakken. De vrije varianttekst is geen keuzelijstbeperking: je kunt rechtstreeks in het tekstveld typen.

### Profielschaal

Vul bij **Profielschaal** bijvoorbeeld `Auto` of `1:200` in en kies **Schaal toepassen**. Deze schaal hoort bij het dwarsprofiel. De kaartschaal stel je afzonderlijk in onder Kaarten en profielen.

## 17. Eigen sjabloon, stempel en logo's

### Het ingebouwde DXF-sjabloon aanpassen

1. Open **Instellingen → DXF-export → DXF-sjabloon**.
2. Kies **Ingebouwd sjabloon opslaan…** en bewaar een bewerkbare kopie.
3. Pas de kopie aan in je CAD-programma. Behoud de exportvakken, blokken en herkenningsnamen die SleufBase nodig heeft.
4. Kies in SleufBase **Aangepast sjabloon laden…** en selecteer de bewerkte DXF.
5. Wacht op de controle en klik op **Opslaan** in Instellingen.

SleufBase bewaart een eigen kopie van het geladen sjabloon. **Origineel herstellen** kiest weer het ingebouwde sjabloon; ook die keuze wordt pas actief na Opslaan. Annuleren behoudt de vorige keuze. In het exportvenster kun je onder Meer exportinstellingen eveneens een eigen sjabloon kiezen voor de export.

### Tekeningstempel en contactgegevens

Bij **Tekeningstempel** in het exportvenster vul je onder meer **Gezien**, **Status**, **Versie**, **Referentie Tekeningen**, **Projectnummer** en **TEK.NR.** in. Bewaar het project om deze projectgegevens te behouden.

**Instellingen → DXF-export → Contactgegevens op DXF-bladen** bevat adres, postcode, plaats, telefoon, e-mail en website. Een leeg veld laat die contactregel weg. Het standaard e-mailadres is **contact@techbasebv.nl**. Met **Standaard Techbase-gegevens herstellen** zet je de meegeleverde waarden terug.

### Opdrachtgeverslogo's beheren

1. Open **Instellingen → DXF-export → Opdrachtgeverslogo's**.
2. Klik op **Logo toevoegen…** en kies een PNG- of JPEG-bestand.
3. Geef het logo een herkenbare **Naam** en een **Afkorting op blad** van maximaal 24 tekens, bijvoorbeeld ENEXIS.
4. Kies eventueel het **Standaard opdrachtgeverlogo** en klik op **Opslaan**.

SleufBase bewaart bij Opslaan een eigen kopie van nieuwe afbeeldingen. Je kunt meerdere logo's bewaren, namen en afkortingen aanpassen en logo's uit de keuzelijst verwijderen. **Annuleren** behoudt de bestaande instellingen. De verzameling is ook beschikbaar bij andere projecten; oudere eigen logokeuzes blijven bruikbaar.

### Een logo kiezen voor de export

Open **Meer exportinstellingen → Logo's op de bladen** in het DXF-sjabloonexportvenster. Kies bij **Opdrachtgeverlogo** een ingebouwd of zelf toegevoegd logo. Nieuwe opdrachtgeverslogo's voeg je in Instellingen toe. De afzonderlijke Techbase-logokeuze blijft beschikbaar.

De projectnummerkop op de bladen volgt de gekozen opdrachtgever. Met de afkorting ENEXIS wordt **PROJECTNR. BGM:** bijvoorbeeld **PROJECTNR. ENEXIS:**. Het ingevulde **Projectnummer** blijft behouden.

### CAD-lettertypen

Onder **Meer exportinstellingen** kun je mappen met extra CAD-lettertypen toevoegen. Bij de portable versie moeten de meegeleverde standaardlogo's in `assets/template_logos` naast de applicatie beschikbaar blijven. Eigen fontmappen moeten tijdens het exporteren bereikbaar zijn.

## 18. Andere exports

| Actie | Uitvoer |
| --- | --- |
| Acties → Export kaarten | Losse kaartbeelden van de geladen TIFFs. Kies een uitvoermap; de kaartinstellingen staan onder Instellingen → Kaarten. |
| Acties → Kadastrale export DXF | Een kadastrale overzichtstekening. |
| KickTheMap → Exporteer objectpunten naar DXF | Een DXF met de geladen objectpunten. |
| Acties → Discipline-overzicht naar Excel | Een overzicht op basis van de geladen objecten en ingestelde woordenlijst. |

De sjabloonexport kan daarnaast volgens de ingestelde voorkeur een discipline-overzicht maken. Controleer de gekozen sleuven en labels: de opname en volgorde in de export bepalen welke vakken in dat overzicht terugkomen.

Online achtergronden en aanvullende gegevens moeten beschikbaar zijn tijdens het exporteren. Een offline sjabloonexport gebruikt de geladen gegevens en een witte achtergrond.

## 19. Instellingen, projecten en back-ups

### Instellingen

| Categorie | Belangrijkste onderwerpen |
| --- | --- |
| Kaarten | Werkkaart, naambordjes en kaartuitvoer. |
| DXF-export | Sjabloon, proefsleufweergave, kleuren, rasters, contactgegevens en opdrachtgeverslogo’s. |
| Proefsleuven | Profielen, maaiveld, aanvullende gegevens en uitvoer. |
| KickTheMap | Woordenlijst, materiaalkeuzes en marXact-naamkoppelingen. |
| Back-ups | Autosave, bewaarinstellingen en herstel. |

Klik op **Opslaan** om wijzigingen toe te passen. **Annuleren** behoudt de bestaande instellingen. De instellingen kunnen bij het geopende project horen en worden ook als standaard bewaard; sla het projectbestand daarom eveneens op. Opgeslagen projecten kunnen hun eigen keuzes terugbrengen wanneer je ze opent. Back-upinstellingen gelden voor de hele app.

### Project opslaan en openen

Gebruik **Acties → Project opslaan** of **Project opslaan als**. **Project laden** opent een eerder project. Een geopende of bewerkte sleuf is pas in het bestand bewaard nadat je het project hebt opgeslagen.

Houd de oorspronkelijke bronbestanden bereikbaar: het project kan verwijzen naar externe TIFF- en andere bestanden. Voor verder werken op een andere computer zijn daarom naast het project ook de gebruikte bronbestanden nodig. KickTheMap-inloggegevens worden niet in het projectbestand opgenomen.

### Een back-up herstellen

Open **Instellingen → Back-ups**, stel eventueel Autosave in en gebruik **Autosave laden…** om het hersteloverzicht te openen. Kies de gewenste beschikbare back-up en volg de melding voor eventuele onopgeslagen projectwijzigingen.

Na een geslaagde herstelactie sluit Instellingen en kom je terug op het hoofdscherm. Het herstelde project heeft nog geen nieuwe opslaglocatie. Gebruik **Project opslaan** of **Project opslaan als** en kies een nieuw projectbestand; daarmee werk je verder zonder het back-upbestand te overschrijven. Bij annuleren of een laadfout blijft de huidige situatie beschikbaar.

Automatische back-ups vervangen het bewust opslaan van je werkbestand niet. Je vindt verdere uitleg over de programmamap en gebruikersgegevens in [Installeren en bijwerken](PORTABLE.md#instellingen-en-bestanden-bewaren).

## 20. Bediening en sneltoetsen

| Handeling | Waar en waarvoor? |
| --- | --- |
| F1 | Info en handleiding openen. |
| Ctrl+L | Locatie zoeken. |
| Pijl omhoog/omlaag en Enter | Een zoeksuggestie kiezen en gebruiken in Locatie. |
| Escape | Een actieve teken- of selectiehandeling annuleren waar die modus dit aanbiedt. |
| Ctrl+klik / Shift+klik | Meervoudige selectie; zie het verschil tussen hoofdschermlijsten en de joblijst. |
| Delete / Backspace | Selectie verwijderen in een actieve Proefsleuven- of Overlays-lijst, wanneer geen venster of popup de actie blokkeert. |
| Ctrl+Shift+U | Aanvullende hulpmiddelen tonen of verbergen. Deze zijn ook via rechtsklikken op Instellingen bereikbaar. |

Met een rechtsklik op een proefsleuf krijg je de beschikbare acties voor die bron. Let bij kaartgereedschappen op de statusmelding: de linkermuisknop kan dan een punt kiezen, een lijn tekenen of een selectievak maken.

## 21. Problemen oplossen

| Wat zie je? | Wat kun je doen? |
| --- | --- |
| Een sleuf of overlay is niet in beeld | Zet het vinkje aan en dubbelklik op de regel. Gebruik daarna eventueel Zoom naar extent. |
| Een naambordje ontbreekt | Controleer Proefsleufnaam. Zoom of verschuif om ruimte voor het bordje te maken. |
| De verkeerde kabel is geselecteerd | Open Lijst, klap een eventuele groep open en kies de juiste kabelregel, of gebruik de wisselknop. Een selectielijn kan meerdere kruisende kabels vinden. |
| De jobs zijn verdwenen uit de lijst | Wis de filters en controleer of Alleen geselecteerd, Geladen jobs of Downloadbaar aanstaat. Vernieuw zo nodig. |
| Open browser is uitgeschakeld | Selecteer precies één job, controleer de aanmelding en wacht tot een lopende taak klaar is. Offline cache is geen actieve aanmelding. |
| De joblijst werkt, maar de website vraagt opnieuw om aanmelden | De jobwebsite kan een eigen aanmelding vereisen. Rond de aanmelding in het aparte venster af en open de job opnieuw. |
| Je kunt op de jobskaart niet verschuiven met links | Links tekent het selectievak. Sleep met midden of rechts om te verschuiven. |
| Wijzigingen uit KickTheMap staan nog niet in SleufBase | Sla eerst op op de KickTheMap-pagina en gebruik daarna Objecten opnieuw laden in Jobs. |
| De normale GeoTIFF-download lukt niet | Lees Downloadfouten. Probeer Meer → Handmatige GeoTIFF vastleggen en wacht op de gereedmelding voordat je op de website downloadt. |
| Handmatige vastlegging heeft niets opgehaald | Controleer of de juiste job open is en of de twee minuten wachttijd nog loopt. Start de vastlegging opnieuw als die is verlopen. |
| De marXact-knop Importeren blijft uitgeschakeld | Geef iedere naam een bekend type of kies Negeren. Controleer ook de reeds ingevulde koppelingen. |
| Een kleur bij een meetobject wijkt af | Controleer de gekoppelde discipline en de kleurinstellingen. Een eigen objectkleur heeft voorrang. |
| De verkeerde sleuf komt in de sjabloonexport | Controleer Meenemen en de GeoTIFF-bronkoppeling. De oranje regelselectie bepaalt niet wat wordt geëxporteerd. |
| Variant wordt niet toegepast | Selecteer de bedoelde, meegenomen regels en klik op Nummer/variant toepassen. Gebruik voor eigen tekst bijvoorbeeld B1 of -2; AUTO heeft een vaste betekenis. |
| Een profiel heeft niet de gewenste schaal | Stel Profielschaal in. De kaartschaal verandert alleen de locatiekaart. |
| De exportkaart is wit | Controleer of Offline of de achtergrond Wit is gekozen. Voor een online achtergrond moeten de benodigde kaartgegevens kunnen worden opgehaald. |
| De DXF mist afbeeldingen in CAD | Zet de complete export met de bijbehorende afbeeldingen en oorspronkelijke mappenstructuur bij elkaar. |
| Nieuwe sjabloon- of materiaalkeuzes worden niet gebruikt | Klik op Opslaan in Instellingen. Open een KickTheMap-job opnieuw om zijn keuzelijsten met de instellingen op te bouwen. |
| De herstelde back-up vraagt om een bestandsnaam | Kies een nieuwe projectlocatie. Het herstel overschrijft de back-up niet als werkbestand. |
| Je ziet andere knoppen dan in de handleiding | Controleer de versie onder Info en of je de juiste SleufBase Rust-snelkoppeling hebt gestart. In smalle vensters worden sommige knopnamen korter en staan meer acties onder Meer. |

Lees bij een fout de volledige melding. Noteer bij het melden van een probleem de versie uit **Info**, de gebruikte bronsoort, de gekozen handeling en de fouttekst. Daarmee is duidelijk in welke stap het probleem ontstaat.


## Meerdere LAS/LAZ-scans en bovenaanzicht

Selecteer meerdere scans via **Puntenwolk → LAS/LAZ openen**. De bestanden komen na elkaar in het puntenwolkoverzicht; openen van de Potree-editor is niet verplicht. Kies later **Bewerken** om punten of polylijnen te tekenen. Met een begin- en eindpunt plaatst **Inladen** de proefsleuf rechtstreeks op de kaart. **Meer → Inladen met opties** biedt de eerdere detailkeuzes.

Voor een scan in RD New maakt SleufBase een GeoTIFF-bovenaanzicht uit de echte puntkleuren of intensiteit/hoogte. Alleen de scan vormt het raster: getekende punten, lijnmarkeringen en labels worden niet gefotografeerd. Dit GeoTIFF wordt bij het inladen als kaartlaag gebruikt en kan bij de DXF-sjabloonexport als raster dienen.

Bij LAS/LAZ-bovenaanzichten wordt de GeoTIFF-richting bepaald door de lijn van **Beginpunt** naar **Eindpunt** in Potree. De GeoTIFF blijft op RD-coördinaten liggen; na het verplaatsen van een eindpunt wordt de GeoTIFF bij opnieuw inladen vernieuwd. Typekleuren van punten en lijnen veranderen direct tijdens bewerken.
