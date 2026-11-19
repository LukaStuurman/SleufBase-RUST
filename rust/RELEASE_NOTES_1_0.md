# SleufBase Rust 1.0.42 — maaiveldtekst onder lijn, beide richtingen standaard

## DXF-sjabloonexport

- De tekst **Maaiveld: … N.A.P.** staat in het dwarsprofiel voortaan **onder** de maaiveldlijn, met zichtbare afstand tot de lijn. Dit geldt voor normale en omgekeerde richting en voor profielen met handmatig ingevulde of automatisch aangevulde maaiveldhoogtes.
- De afzonderlijke referentielijn, N.A.P.-band, kabel-/leidingsymbolen, profielwaarden en werkelijke hoogtes blijven verder ongewijzigd.
- Bij het openen van het venster **Proefsleuven sjabloon DXF** staat **Beide kijkrichtingen** standaard aangevinkt. Ook het alternatieve sjabloonvenster begint met **Beide richtingen (Normaal / Reverse)** aangevinkt, zelfs als een ouder instellingenbestand daarvoor nog `false` bevat.
- Het vinkje blijft per export uitzetbaar. Het opstarten van een nieuw exportvenster begint opnieuw met beide richtingen aan. De algemene standaardwaarde voor nieuwe installaties is eveneens bijgewerkt.

## Controle

De Windows-release wordt gecompileerd en de portable ZIP en installer worden gecontroleerd via GitHub Actions. De volledige geautomatiseerde testset is volgens de eerdere afspraak overgeslagen. De gewijzigde DXF-tekstpositie is niet interactief in AutoCAD met een werkelijk uitgevoerde sleuf geverifieerd.
