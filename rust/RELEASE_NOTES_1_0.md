# SleufBase Rust v1.0.57 — directe offline vernieuwing en scherpte behouden tijdens pan/zoom

## Offline luchtfoto direct opnieuw downloaden

- **Nieuwe knop onder Instellingen → Kaarten:** `Nu nieuwe offline luchtfoto downloaden en gebruiken` haalt op verzoek alle 144 offline detailtegels opnieuw op het gekozen niveau van 10, 25 of 50 MB, ook wanneer automatische halfjaarlijkse vernieuwing uit staat. De knop schakelt de werkkaart direct naar de offline luchtfoto.
- **De overzichtsfoto wordt eveneens vernieuwd:** een nieuwe PDOK-WMS-afbeelding van het landelijke gebied wordt onafhankelijk van de bestaande verloopdatum op de achtergrond opgehaald. Beide bronnen gebruiken RD / EPSG:28992.
- **Keuze wordt bewaard:** het gekozen offline detailniveau en de actieve offline achtergrond worden direct opgeslagen. Andere, nog niet opgeslagen wijzigingen in Instellingen blijven ongewijzigd totdat je zelf Opslaan kiest.
- **Voortgang alleen in Instellingen:** de handmatige download toont in het instellingenvenster hoeveel detailtegels zijn bijgewerkt. De normale statusbalk toont geen laadpercentages, tegelcijfers of herhaalde standaardmeldingen meer.
- **Geen impliciete toestemming voor automatisch bijwerken:** de handmatige knop zet de halfjaarlijkse download niet aan. Die blijft standaard uit totdat de gebruiker hem expliciet activeert.
- **Veilig bij een netwerkstoring:** iedere offline JPEG wordt pas na volledige validatie atomair vervangen. Een mislukte of onderbroken download laat de reeds aanwezige landelijke foto en opgeslagen detailtegels intact.

## Sneller en stabieler PDOK-luchtfoto laden

- **Scherpe pixels niet kwijtraken bij bewegen:** de renderer bewaart bij een geannuleerde aanvraag het geografisch correcte al samengestelde tussenbeeld, inclusief de scherpere tegels die net binnenkwamen. Bij de volgende kleine pan/zoom gebruikt hij dat opnieuw, in plaats van terug te vallen op de laatste volledig afgewerkte, mogelijk grovere uitsnede.
- **Transparante ontbrekende pixels overschrijven de oude kaart nooit:** het herprojecteren van een eerdere gedeeltelijke afbeelding houdt volledig doorzichtige pixels buiten de compositie.
- **Minder onnodig lage-resolutiewerk:** wanneer een eerdere afbeelding nog minstens ongeveer 65% van het nieuwe kaartgebied dekt, worden geen extra grovere parent-tegels samengesteld. De resterende netwerkcapaciteit gaat naar ontbrekende scherpe WMTS-tegels.
- **Groot nieuw kaartgebied:** bij een koude of zeer grote verschuiving blijven de bestaande grovere preview en ingebouwde offline luchtfoto beschikbaar als tijdelijke ondergrond.
- **Statusbalk opgeruimd:** succes- en voortgangsevenementen van de achtergrondkaart worden stil verwerkt. Fouten die opvolging vereisen blijven zichtbaar.

De verbeteringen gelden voor het interactieve kaartscherm; DXF-sjabloonexport en geopende proefsleuven blijven ongewijzigd.

## Validatie

Nieuwe regressiecontroles staan in de code voor het behouden van scherpere pixels in gedeeltelijke WMTS-frames en het gescheiden houden van een handmatige offline download en automatische opt-in. De Windows-release wordt op de bestaande afspraak zonder volledige geautomatiseerde tests en Clippy gebouwd; geoptimaliseerde Windows-compilatie, installer, portable pakket en checksums worden wel gecontroleerd. Praktische prestaties zijn nog niet op de gebruikerswerkplek gemeten.
