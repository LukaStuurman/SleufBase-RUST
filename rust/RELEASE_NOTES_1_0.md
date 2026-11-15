## SleufBase Rust 1.0.3 voor Windows

### KickTheMap-aanmelding onthouden en herstellen

- Bij het starten probeert SleufBase een onthouden KickTheMap-sessie op de achtergrond te herstellen. Een geslaagde automatische aanmelding opent geen extra jobsvenster.
- Zonder opgeslagen browsersessie kan SleufBase de bewaarde rechtstreekse aanmelding gebruiken. Na mislukt sessieherstel blijft duidelijk welk account de gebruiker opnieuw moet bevestigen; de app wisselt niet stilzwijgend naar een ander opgeslagen account.
- Annuleren en een onderbroken verbinding laten geen vastgelopen aanmeldstatus achter.
- Uitloggen schakelt automatisch aanmelden blijvend uit, ook als een ander SleufBase-venster het browserprofiel nog gebruikt. De app meldt duidelijk wanneer opgeslagen gegevens niet volledig konden worden opgeruimd.
- De aanmeldopties en foutmeldingen zijn in gewone taal beschreven. Een verplichte beveiligingscontrole van KickTheMap blijft beschikbaar in de ingebouwde websiteweergave.

### Overzichtelijker virtuele proefsleuven bewerken

- Een brede editor toont de doorsnede en kabels-/leidingenlijst naast de invoer. Op smallere vensters staan deze onderdelen onder elkaar.
- De kop toont de sleufnaam, lengte, breedte en het aantal kabels en leidingen.
- Positie en diepte, type en afmetingen zijn gegroepeerd. De kruishoek blijft direct visueel instelbaar; aanvullende brongegevens staan onder uitklappers.
- Met **Zelfde type toevoegen** worden gebruikerseigenschappen klaargezet voor een volgende kabel. Kies daarna een nieuwe plek. Alleen de voorinvulling openen maakt geen extra kabel aan.
- Herhalen neemt geen absolute hoogte, RD-positie, meetidentiteit of bronreferenties over. Gemeten objecten blijven behouden.

### Duidelijkere DXF-sjabloonexport

- Het gecombineerde exportvenster groepeert projectgegevens, kaarten/profielen en proefsleufkeuze. Aanvullende instellingen staan onder **Meer exportinstellingen**.
- Directe acties nemen alle sleuven mee of sluiten ze uit en zetten dwarsprofielen voor meegenomen sleuven aan of uit.
- De vaste samenvatting toont het aantal sleuven/profielen, de gebruikte kaartachtergrond, kaartkwaliteit en uitvoer.
- Offline export toont duidelijk dat geen luchtfoto wordt opgehaald, terwijl de online achtergrondvoorkeur behouden blijft.
- Ontbrekende geselecteerde bronnen, werkfoto's en logo's en ongeldige instellingen worden vóór het starten van de export gemeld. Uitgesloten sleuven blokkeren de export niet.
- Het openen van het venster verandert geen opgeslagen DPI- of schaalwaarden. De invoer ondersteunt hetzelfde bereik als het projectformaat.

### Beschikbare functies

Handmatig maaiveld invullen, AHN, de scherpe sjabloondetails en de verbeteringen aan kabelinvoer en Python-projectuitwisseling blijven behouden. Gemma is verwijderd.

Gebruik de Windows-installer of portable ZIP. Beide downloads hebben een SHA-256-controlebestand.
