# Aanvullende integratiecontrole 25 oktober 2026

De port en overdracht naar SleufBase-RUST blijven onvoltooid. Deze ronde bouwt
voort op de vorige controle en corrigeert een eerder gemiste actieve opstartpatch.
Alle volledige acceptatievoorwaarden blijven vereist.

## Getoetste wijzigingen

1. AHN publiceert ieder punt direct, ook vóór een volgende langzame aanvraag of
   fout. Eén worker en een resultaatkanaal van één item begrenzen de wachtrij.
   Bron-/punt-/metadata-/projectcontroles behouden handmatige en gemeten hoogtes.
   Annuleren van een geblokkeerde HTTP-aanvraag geeft de caller direct vrij;
   maximaal vier gedeelde transporttaken kunnen hun sockettimeout afwachten,
   zonder latere cachepublicatie of een volgend zoekvenster.
2. De oorspronkelijke WMS-route heeft een gedeelde onveranderlijke LRU met
   24 items/96 MiB, drie pogingen met 0,4/0,8 seconde terugval en progressieve
   tegelbeelden. De desktop bewaart hoogstens één volgend voorbeeld; project- en
   bronwissels annuleren de oude taak. Volledige cachehits leveren direct het
   eindbeeld. Hardware-afhankelijke quota en parallelle timing zijn nog geen
   prestatiepariteitclaim.
3. Oorspronkelijke bladvolgorde, uitsluitingen, lege vakken, PS-nummers,
   varianten en schaal worden actief hersteld. De gewone sjabloonmenuactie opent
   die gemengde indeling. Exacte archiefindices behouden verschillende onbekende
   velden van lege vakken; virtuele bronidentiteit blijft ook na hernoemen of
   dubbele job-/naamparen herkenbaar. Algemene/projectvoorkeuren blijven effectief
   tot de eerste beoordeelde native export. Native opslaan en actieve
   Python-terugconversie worden met wijzigingen gecontroleerd.
4. Een opgeslagen handmatige KickTheMap-aanvraag kan voor één gekozen job worden
   geïmporteerd. GET/POST, multipart/form/JSON, eigen antwoordpad, CSRF-verversing
   en oorspronkelijke fallback worden getoetst. Alleen een nieuw voorbeeld met
   ruimtelijke metadata en leesbare pixels promoveert de downloadwijze; een
   TIFF-header alleen volstaat niet. Opslag hoort bij de vaste sessie-eigenaar.
   Automatisch vastleggen vanuit het browservenster en alle accountvarianten
   blijven open.
5. De actieve `template_wireframe_context_patch` v3 vergroot lokale context
   exact tienmaal en behoudt verre vakken individueel. De vorige 35–75 m/maximaal
   twaalf-vakkenverwachting beschreef de ongepatchte helper en is vervangen door
   het werkelijk geïnstalleerde contract. Template-WFS gebruikt de oorspronkelijke
   350 m-tegels met 8 m overlap en een afzonderlijk begrensd queryplan; de
   interactieve kaartlimieten blijven gelden voor interactieve aanvragen.
6. Gekoppelde MTEXT-kolommen worden als één groep gekopieerd, met eigen child-
   handles, eigenaars en dictionaries. De oorspronkelijke geschreven geometrie,
   kolomtekst, plaatsing, achtergrond en R2018 embedded tags blijven behouden.
   Native behoudt bovendien kolommetadata die ezdxf bij oorspronkelijke kopieën
   verloor. Dit bewijst geen volledige MTEXT-kolomweergave op de kaart.

## Bewijs

- De huidige set omvat **725 unieke Rust-tests over 150 targets**, waaronder
  241 kern- en 111 desktoptests. De volledige all-targets-run slaagde met 724;
  daarna slaagden alle 111 desktoptests opnieuw, inclusief de toegevoegde gewone
  menuactie voor herstelde indelingen. Clippy slaagt voor alle targets, met
  bestaande en stijlwaarschuwingen. De daadwerkelijke executable en acht drivers
  zijn opnieuw gebouwd.
- Zes oorspronkelijke generators reproduceren tien referentiebestanden
  bytegelijk. De geselecteerde bronfuncties/bytecode draaien geïsoleerd; de oude
  app, instellingen en live-renderpatch worden niet gestart. Daarnaast slagen
  25 bestaande Python-tests met de werkelijk actieve templatepatches en een
  expliciete importblokkade voor app/instellingen/live-renderinitialisatie.
- Acht groepen daadwerkelijke uitvoer slagen onafhankelijk:

| Uitvoer | Vergelijking |
| --- | --- |
| MTEXT-kolommen | Acht DXFs, zestig oorspronkelijke parents, 54 herstelde oorspronkelijke metadataverliezen; volledige bronpayloads, zelfstandige owners en fout-/reparatievrije CAD-audits |
| Verse diensten | Vijf gepaarde DXFs, 155 oorspronkelijk geschreven overzichtsobjecten, actieve v3-context, dienstvolgorde, union/deduplicatie en terrein-fallback |
| XCLIP | Drie DXFs, dertig oorspronkelijke INSERT-kopieën en onafhankelijke filter-/dictionary-/attribuutgrafieken |
| Schalen | Vier projectplanner-DXFs en 209 complete oorspronkelijke profielobjecten |
| Maatvoering | Drie DXFs en vijftig oorspronkelijke maatvoeringen met eigen geometrie-/metadatagrafieken |
| Gewone workers | Acht varianten met datum/projectnaam, geselecteerde punten, opmerkingen, rotatie, duurzame PNGs en bronteruggave |
| Voorbereide uitvoer | Vijf exports, acht complete profielen en 26 PNGs; echte CLI, relatieve manifestpaden, Excel en weigering van overschrijven |
| Overzicht | Acht zelfstandige plus acht gepaarde exports; beide series 91 complete oorspronkelijke objecten/twaalf affine TIFFs, bytegelijke bronbeelden en wederzijdse reactors |

Een bewust gewijzigde child-dictionarypayload wordt door de nieuwe kolomcontrole
afgekeurd op inhoud, ook als de native eigenaarsgrafiek geldig blijft.

## Windows-pakket en resterend werk

De nieuwe pakketbouwer publiceert één complete map atomair, met een gecontroleerde
ZIP, SHA-256 en een manifest voor alle dertien vereiste bestanden. Zes negatieve
en reproduceerbaarheidstests controleren gewijzigde inhoud, ontbrekende/extra/
dubbele namen, verkeerde architectuur en behoud van bestaande uitvoer. Het echte
ontwikkel-EXE is verpakt en `--help` slaagt vanuit die map met alleen de normale
Windows-systeempaden, zonder compiler- of Pythonpad.

Een afzonderlijke native previewinstaller heeft een eigen installatie-identiteit,
map en snelkoppelingen en behoudt de oorspronkelijke standaard Nederlandse wizard.
De statische contractcontrole slaagt. De nieuwe Windows-CI moet de daadwerkelijke
installer nog compileren; lokaal is geen Inno Setup geïnstalleerd. Er is niets
geïnstalleerd op het gebruikerssysteem. Updater/rollback, signing en zelfstandige
overdracht/publicatie blijven open.

De actuele oorspronkelijke testinventaris bevat **76 modules/386 testfuncties**.
Deze inventaris is geen dekkingsclaim: alle relevante contracten uit deze superset
van de historische 72 modules vereisen expliciet bewijs. Volledige conversie,
alle oorspronkelijke gebruikersacties, bron-/profielhydratie, provider-/account-
en GPU/modelvarianten, representatieve projecten en echte CAD-acceptatie blijven
vereist. Overviews/pyramides/reprojectie zijn niet aangetroffen in de onderzochte
actieve TIFF/WMS-route; deze bronbevinding sluit de overige kaart- en eerder
genoteerde uitbreidingsvoorwaarden niet af. Cyclomedia/StreetSmart blijven de
expliciete uitzondering op gebruikersverzoek.

Lokale logs/bewijs onder `%LOCALAPPDATA%\SleufBaseDev`:
`root-current-full-reviewed.log`, `root-current-desktop-final.log`,
`root-current-clippy.log`, `root-current-production-build.log`,
`root-original-25-contracts-active-startup.log`,
`high-current-exports-walmd3h7/proof.json` en
`root-current-packaged-cli.log`. De vorige revisie `6ee6fcd` slaagde op alle drie
Windows-workflows; dat bewijst niet de CI-uitkomst van deze nieuwe wijzigingen.
