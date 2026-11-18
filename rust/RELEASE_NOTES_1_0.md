# SleufBase Rust 1.0.30 — lokale LAS/LAZ-puntenwolkeditor

Scans van andere leveranciers kunnen rechtstreeks in SleufBase worden verwerkt met een eigen Potree-editor. De installer en portable ZIP bevatten de viewer en converter; een KickTheMap-account is hiervoor niet nodig.

## Nieuw

- LAS- en LAZ-bestanden openen via het menu Puntenwolk of slepen naar het hoofdvenster.
- Lokale conversie met voortgang en annuleren, gevolgd door een ingebouwde WebView2/Potree-editor.
- Punten en polylijnen op de puntenwolk tekenen, verplaatsen, benoemen en verwijderen, met ongedaan maken en opnieuw uitvoeren.
- Materiaal, diameter en bundel/omschrijving per getekend object bewaren.
- Getekende objecten opslaan in het Rust-project en exporteren naar DXF met de originele XYZ-coördinaten.
- RD-meetobjecten opnemen op de kaart. Andere coördinatenstelsels blijven in hun oorspronkelijke stelsel en worden niet automatisch als RD behandeld.
- Een gemeten sleufas kiezen en geselecteerde meetpunten/kabelkruisingen overnemen naar de bestaande proefsleufbewerker en DXF-sjabloonexport.
- Hetzelfde bronbestand opnieuw koppelen en de lokale weergave opnieuw opbouwen zonder de opgeslagen tekenobjecten te verliezen.

## Gebruik

1. Open **Puntenwolk → LAS/LAZ openen** en controleer naam en coördinatenstelsel.
2. Teken in de editor punten en lijnen en vul hun eigenschappen in.
3. Sluit de editor en bewaar het project via **Acties → Project opslaan**.
4. Gebruik **Puntenwolken beheren → Maak proefsleuf** voor verwerking in een proefsleuf, of **Objecten naar DXF** voor de volledige getekende geometrie.

Bewaar bij het overzetten van een project ook de oorspronkelijke LAS/LAZ. De puntenwolkeditor wijzigt het bronbestand niet. Kaart- en proefsleufintegratie vereisen RD New in meters; er vindt geen automatische herprojectie plaats.

## Controle

Deze release wordt gebouwd en verpakt met de vastgelegde Potree- en PotreeConverter-versies en SHA-256-controles. De geautomatiseerde tests worden op verzoek overgeslagen. Praktijkcontrole met eigen LAS/LAZ-bestanden en WebView2 op Windows blijft nodig.
