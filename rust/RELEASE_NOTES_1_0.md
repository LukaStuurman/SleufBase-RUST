## SleufBase Rust 1.0.1 voor Windows

### Sjabloon-DXF export
- Eén uniform exportvenster voor KickTheMap-jobs, GeoTIFF-proefsleuven en virtuele proefsleuven, ongeacht in welke volgorde ze zijn toegevoegd.
- Virtuele proefsleuven blijven in de exportselectie zichtbaar, ook wanneer oude sjablooninstellingen of een rasterlayout worden hersteld.
- Nieuwe online exports gebruiken standaard de PDOK-luchtfoto als achtergrond, met transparant weergegeven virtuele sleuf.
- De offline modus haalt geen nieuwe luchtfoto op; het exportvenster maakt dit duidelijk.

### Gecombineerde verbeteringen uit PR #14, #15, #17 en #18
- Native KickTheMap-inlogformulier met Windows-DPAPI-beveiligde opslag en automatische heraanmelding. Verplichte verificatie vindt zo nodig binnen SleufBase plaats.
- Verbeterde herkenning van KickTheMap-GeoTIFF-coördinaten (onder meer Nederweert).
- Stabiel bovenaanzicht tijdens het aanpassen van kabelkruishoeken.
- Veilige accountwissel, annulering van aanmelden en opruimen van sessiegegevens.

Gebruik de Windows-installer of de portable ZIP. De SHA-256-bestanden kunnen gebruikt worden om de downloads te controleren.
