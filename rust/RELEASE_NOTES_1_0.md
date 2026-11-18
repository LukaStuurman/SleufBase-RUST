# SleufBase Rust 1.0.31 — punten plaatsen in de Potree-editor hersteld

Deze versie verhelpt de Potree-fout `Cannot read properties of undefined (reading getCenter)` bij het plaatsen of selecteren van een meetpunt.

## Hersteld

- Meetpunten en de hoekpunten van polylijnen gebruiken de eigen objectselectie van SleufBase. Zij worden niet meer onbedoeld aangemeld bij Potrees gereedschap voor het verplaatsen en schalen van volumes.
- Een klik op een meetpunt selecteert het bijbehorende object zonder dezelfde klik opnieuw aan de algemene Potree-selectie door te geven.
- Het plaatsen van een punt wordt pas afgerond nadat Potree de muisklik en het neerzetten van de marker heeft afgehandeld.
- Dezelfde markerafhandeling geldt voor nieuw getekende, opnieuw geopende en via ongedaan maken/opnieuw herstelde objecten.

## Bijwerken

Sluit SleufBase en installeer versie 1.0.31. Open daarna je bestaande project of LAS/LAZ-bestand opnieuw. Het projectformaat en de voorbereide puntenwolken blijven compatibel met versie 1.0.30; opnieuw converteren is voor deze correctie niet nodig.

## Controle

De correctie is beoordeeld tegen de meegeleverde Potree 1.8.2-code. JavaScript-syntax, Rust-compilatie en de Windows-pakketten worden gecontroleerd. Geautomatiseerde tests blijven volgens afspraak overgeslagen. De interactieve Windows-bediening met eigen scanbestanden is niet uitgevoerd.
