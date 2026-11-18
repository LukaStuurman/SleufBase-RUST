# SleufBase Rust 1.0.39 — één puntenwolkproefsleuf op de kaart

## Nieuwe werking voor ingeladen LAS/LAZ

- **Geen dubbele visualizer:** een puntenwolkproefsleuf wordt niet langer als zwarte virtuele rechthoek met gekleurde, dwarsgetekende markeringen boven op de orthofoto getekend.
- **Eén vermelding links:** het menu **Proefsleuven** toont de gekoppelde orthofoto als één laag. De afgeleide virtuele proefsleuf is uitsluitend nog het interne meetmodel voor kabels, dieptes, dwarsprofielen en DXF-export; zij verschijnt niet als tweede bestand.
- **Direct op de kaart:** de knop **Inladen** neemt de getekende begin-/eindpunten en kabel-/leidinggegevens uit de Potree-editor over. Zodra de orthofoto is gemaakt, wordt die automatisch als echte RD-kaartlaag geselecteerd. De aparte gewone virtuele sleufeditor wordt niet geopend.
- **Foto direct zichtbaar:** het verschijnen of opnieuw opbouwen van een scanorthofoto ververst zowel de basisraster-preview als de cameragebonden detailweergave. Eerder gedraaide of oudere textures worden niet hergebruikt.
- **Oudere projecten:** een eerder opgeslagen selectie van de virtuele LAS-sleuf verwijst bij het openen naar de gekoppelde orthofoto.
- **Eén logische verwijderactie:** verwijder je een orthofoto van een LAS/LAZ-sleuf uit de projectlijst, dan worden ook de bijbehorende afgeleide meetmodellen en in-project scanoverlays verwijderd. De oorspronkelijke LAS/LAZ en de persoonlijke puntenwolkcatalogus blijven bewaard.
- **Overige sleuven ongewijzigd:** handmatig gemaakte virtuele proefsleuven en MarXact-sleuven blijven hun bestaande weergave en bediening gebruiken.

De verbeteringen uit 1.0.38 blijven beschikbaar, waaronder de scanorthofoto met de naam van de puntenwolk, maximaal 8.192 pixels langs de langste zijde en 36 megapixels, de juiste RD-oriëntatie en behoud van originele 3D-polylijnen in de DXF.

## Controle

De normale Windows-compiler, installatiepakketcontrole en releaseworkflow blijven actief. Volledige geautomatiseerde tests zijn voor deze release op verzoek overgeslagen. De kaartbediening met het echte scanbestand is niet interactief op de Windows-computer getest.
