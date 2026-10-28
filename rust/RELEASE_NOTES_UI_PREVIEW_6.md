# SleufBase Rust – UI preview 6

## WebView2 in de installer

De Windows-installer bevat nu de volledige Microsoft WebView2 Evergreen x64-runtime. Deze wordt alleen geïnstalleerd als een bruikbare runtime ontbreekt. Tijdens installatie is hiervoor geen aparte internetdownload nodig. De installer is daardoor ongeveer 200 MB groter. De portable ZIP blijft klein en vereist een reeds geïnstalleerde WebView2-runtime.

De build controleert de Microsoft-handtekening van de runtime. Een mislukte installatie of vereiste herstart wordt gemeld voordat SleufBase wordt geïnstalleerd. De gedeelde runtime wordt bij het verwijderen van SleufBase behouden.

## DXF-sjabloonexport

- De gewone exportkeuze neemt bij een nieuwe export zowel geladen TIFF’s als virtuele proefsleuven mee naar hetzelfde controlevenster.
- Het ingebouwde sjabloon blijft de standaard.
- Het exportvenster legt uit wat nog ontbreekt, zoals een uitvoer-DXF, PS-nummer of TIFF-koppeling. Een bestaande uitvoer wordt beschermd.
- Als een export niet kan starten, blijven de ingevulde keuzes in het venster behouden.
- Een regressietest maakt werkelijk een gecombineerde DXF met TIFF en virtuele sleuf, controleert de afbeeldingen en controleert bescherming tegen overschrijven.

## Virtuele proefsleuf

- Eerder ingevoerde diepte is bij opnieuw bewerken zichtbaar. Omschakelen naar een handmatige hoogte verwijdert de maaiveldkoppeling, ook wanneer de hoogte zelf gelijk blijft.
- Bewerken en verwijderen staan onder de objectregel, zodat lange namen deze knoppen niet buiten een smal venster duwen. Bundelaantallen worden in formulier en lijst hetzelfde gelezen.
- Het venster maakt duidelijk wanneer een bestaande kabel/leiding wordt bewerkt. Toevoegen is alleen actief op het invoertabblad.

## Controle en beperking

De release wordt vanaf gecontroleerde main-bronnen gebouwd. Windows-tests, Clippy, browsercontracten, pakketverificatie, installer installeren/bijwerken/verwijderen en geïsoleerde tests van de WebView2-installatielogica moeten slagen voor publicatie. Er is geen Computer Use gebruikt.

Een echte installatie van een ontbrekende WebView2-runtime op een schone computer is niet handmatig uitgevoerd; die fout-, herstart- en succesroutes worden met een geïsoleerde installerfixture gecontroleerd. Echte KickTheMap-accountaanmelding en reCAPTCHA blijven niet met een gebruikersaccount geverifieerd.
