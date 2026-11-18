# SleufBase Rust 1.0.34 — automatische objectnamen en KickTheMap-stijl puntenwolkenlijst

## Verbeterd

- Als je in de puntenwolkeditor het type van een automatisch benoemd punt of lijnobject wijzigt, verandert de objectnaam direct mee naar het nieuwe type en een uniek volgnummer. De eigenschappenlijst en Potree-marker tonen meteen de nieuwe naam; ongedaan maken en automatisch opslaan blijven werken.
- Bewust handmatig aangepaste namen blijven behouden wanneer het type wijzigt.
- Het overzicht **Puntenwolken beheren** gebruikt nu de **SleufBase Jobs / KickTheMap-lijststijl**: compacte afgeronde rijen, afwisselende lichte achtergronden, oranje selectie, statuslabels, responsieve kolommen, een vaststaande kop en soepel scrollen.
- Net zoals bij KickTheMap krijg je een **Lijst / Kaart**-keuze, een menu **Filter / selectie**, sorteerbare kolommen en indien er ruimte is een filterveld per kolom. In het geselecteerde scanpaneel staan de knoppen **Inladen** en **Bewerken** in dezelfde oranje/witte knopstijl.
- De scanlocatie blijft zichtbaar op de kaart bij bevestigde RD-coördinaten. De volledige XYZ-coördinaten en het bronbestand blijven leesbaar bij de geselecteerde scan en in de tooltip.
- Het puntenwolkoverzicht blijft permanent bewaard, onafhankelijk van het huidige project. De oplossingen voor lokale Potree-laadfouten, puntselectie en het gebruik van begin/eindpunten uit eerdere versies blijven aanwezig.

## Bijwerken

Sluit SleufBase en installeer 1.0.34. Bestaande catalogusrecords, metingen, LAS/LAZ-cache en projecten worden niet verwijderd of omgezet. Selecteer een scan in de nieuwe lijst en gebruik **Bewerken** of **Inladen** onderin.

## Controle

Windows-releasebouw, pakketverificatie en controlesommen worden door de releaseworkflow gecontroleerd. De geautomatiseerde testset blijft op verzoek uitgeschakeld. De interactieve bediening in de lokale Windows-installatie is niet uitgevoerd.
