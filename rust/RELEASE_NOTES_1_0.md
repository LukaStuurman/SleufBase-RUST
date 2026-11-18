# SleufBase Rust 1.0.32 — lokaal laden van de puntenwolkeditor hersteld

Deze versie herstelt een Windows-fout in de lokale verbinding naar de Potree-editor. Die kon grote viewerbestanden onvolledig doorgeven en een lege puntenwolkweergave met een algemene opstartmelding veroorzaken.

## Hersteld

- De lokale server zet iedere geaccepteerde verbinding expliciet in de juiste lees- en schrijfmodus. Op Windows bleef anders de nonblocking-modus van de listener actief, waardoor een bestandsoverdracht voortijdig kon stoppen.
- Een tijdelijke, door de browser afgebroken verbinding stopt niet meer de volledige lokale server.
- Opstartfouten bewaren de oorspronkelijke script- of modulefout en de betrokken bestandsnaam. Een mislukte resource wordt niet meer zonder bewijs als een ontbrekend installatieonderdeel beschreven.
- De verbeterde puntselectie en muisafhandeling uit 1.0.31 blijven aanwezig.

## Bijwerken

Sluit SleufBase, installeer 1.0.32 en open je bestaande project of puntenwolk opnieuw. De bestaande projectgegevens en voorbereide LAS/LAZ-weergaven blijven bruikbaar; deze correctie vereist geen nieuwe conversie.

## Controle

De Windows-socketafhandeling is beoordeeld tegen de officiële Microsoft- en Rust-documentatie. Het gepubliceerde 1.0.31-pakket is statisch gecontroleerd: de runtimebestanden waren aanwezig en hun controlesommen klopten. Deze release krijgt Rust- en Windows-bouwcontrole, JavaScript-syntaxcontrole en pakketverificatie. Geautomatiseerde tests blijven volgens afspraak overgeslagen; er is geen interactieve Windows-praktijkcontrole uitgevoerd.
