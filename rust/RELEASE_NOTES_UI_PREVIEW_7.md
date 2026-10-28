# SleufBase Rust – UI preview 7

## Jobs openen en bestanden laden

- De jobdetails gebruiken nu dezelfde opslagdatum en gebruikersidentiteit als de originele SleufBase. Ook jobs met alleen een prefix, een afwijkende datumopmaak of andere schrijfwijze van UserId worden ondersteund.
- Een fout in de rechtstreekse downloadaanvraag blokkeert de ingelogde sessie niet meer. De aanvullende opslagroute uit de originele SleufBase is overgenomen voor wanneer de gewone bestandslink of download faalt.
- De opslagroute gebruikt uitsluitend de jobidentiteit uit de actuele, bevestigde joblijst. Opslagtoegangsgegevens blijven in het geheugen van de actieve sessie en worden niet in projecten of downloadstrategieën opgeslagen.
- GeoTIFF’s en objectbestanden worden vóór het laden gecontroleerd. Bestaande bestanden blijven behouden; annuleren en herstel van een downloadstrategie behouden hun eigen controles.

## Gewoon werken in het SleufBase-jobsvenster

- **Laad geselecteerde jobs** haalt objecten en GeoTIFF’s op en laadt ze rechtstreeks in SleufBase. Dubbelklikken op een job doet hetzelfde.
- Het aanmeldvenster wordt na geslaagde sessieovername verborgen en het jobsvenster wordt geopend. De sessie blijft beschikbaar voor downloads. Het venster kan via Account weer worden getoond.
- Website-details zijn een afzonderlijke optie onder **Meer opties en probleemoplossing**. Ze zijn niet nodig om jobs in SleufBase te laden.
- De joblijst krijgt meer ruimte. Filters en selectieopties staan bij elkaar; laden en annuleren blijven bereikbaar bij kleine vensters. Lange meldingen vergroten het venster niet door muisbewegingen.
- Tijdens een download blijft de live joblijst zichtbaar. Tekstsorteersleutels worden eenmaal per job berekend in plaats van bij elke vergelijking.

## Controle

Vergeleken met originele SleufBase main `0257f1d6bd10f90741bda2bbeff9522c210554bc`. Regressietests omvatten joblinks, echte lokale HTTP-foutantwoorden, fallback en annuleren, actuele opslagidentiteit, het originele versleutelde protocol, een onafhankelijke botocore-handtekening en het officiële AWS-testvoorbeeld. UI-tests controleren dubbelklikken, kleine vensters, zichtbare annuleerknoppen en stabiele venstergrootte bij lange meldingen. Er is geen Computer Use gebruikt.

De Windows-installer behoudt de volledige WebView2-runtime uit preview 6. Het portable pakket bevat nu vijftien gecontroleerde bestanden, inclusief de notices voor de nieuwe protocolafhankelijkheden.

De werkelijke download met een gebruikersaccount is niet uitgevoerd. Aanmelden en reCAPTCHA blijven handmatige stappen wanneer KickTheMap daarom vraagt. Deze preview levert gecontroleerde correcties op de gevonden foutpaden; bevestig de uitkomst ook met eigen jobs.
