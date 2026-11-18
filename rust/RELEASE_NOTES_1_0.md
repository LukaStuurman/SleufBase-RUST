# SleufBase Rust 1.0.41 — WGS 84 in de puntenwolkeditor en schone DXF-kaart

## Coördinatenstelsels

- **WGS 84 toegevoegd** bij het toevoegen en beheren van puntenwolken. De keuzelijst bevat nu Onbekend / uit bestand lezen, RD New (EPSG:28992), **WGS 84 (EPSG:4326 / EPSG:4979)** en Ander / lokaal.
- Metadata met EPSG:4326 en EPSG:4979 wordt bij automatische herkenning als WGS 84 geselecteerd. Als een bestand een afwijkende EPSG-code vermeldt, kan die scan niet onterecht als WGS 84 worden opgeslagen.
- De Potree-editor toont bij WGS 84 geografische lengte- en breedtegraad in graden. Hij combineert de horizontale graden niet langer met Z-hoogte tot een misleidende 3D-afstand.
- **Belangrijk:** het kiezen van WGS 84 voert geen herprojectie uit. WGS 84-bronnen blijven toegankelijk voor bekijken, intekenen en object-DXF met de oorspronkelijke XYZ. Rechtstreeks plaatsen op de Nederlandse RD-kaart en de bijbehorende orthofoto vereisen nog steeds correct gemeten en naar RD New omgezette coördinaten.

## DXF-sjabloonexport

- De zwarte proefsleufrechthoek wordt bij een LAS/LAZ-orthofoto niet meer over de **kaart met legenda** getekend. De afzonderlijke referentiepuntmarkering en de getekende kabel-/leidinglijnen blijven behouden.
- De kaartweergave van andere virtuele en MarXact-proefsleuven verandert niet.

## Controle

De normale Windows-build, portable pakketcontrole en installerbouw blijven actief. Volledige geautomatiseerde tests worden volgens de eerdere afspraak overgeslagen. De 3D-interactie is niet interactief getest op de Windows-computer met het originele scanbestand.
