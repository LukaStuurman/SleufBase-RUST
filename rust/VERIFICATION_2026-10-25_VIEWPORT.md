# Aanvullende controle van kaart, profielbronnen en sloteditor

De volledige Rust-port en de overdracht naar SleufBase-RUST blijven onvoltooid.
Alle zes acceptatiepakketten in PARITY.md blijven vereist. Deze ronde gebruikt
agents op laag effort en afzonderlijke review op hoog.

## Wijzigingen

- De kaartachtergrond volgt de werkelijke camera, canvasgrootte en schermresolutie.
  Eén blijvende worker begrenst aanvragen en voorbeelden; verouderde generaties
  worden geannuleerd. Bron-/projectwissels, zoomen en uitschakelen worden getoetst.
  De oorspronkelijke 45/160 ms planning, dekking/resolutie en kadastrale grens
  blijven leidend; die grens gebruikt logische pixels, ook bij hoge scherm-DPI.
- WMS gebruikt hardware-afhankelijke cachequota. OSM heeft een blijvende luie
  HTTP-client, begrensde onveranderlijke tegelcache, gedeelde tegelsloten,
  brongetrouwe vervaltijden en drie pogingen. Gewijzigde bestanden invalideren
  geheugen; een mislukte bestandsopslag vernietigt de verse geheugenkopie niet.
- Fysieke TIFF-profielen gebruiken hun eigen lokale jobFeatures-koppeling.
  Een eigenaar-gecontroleerde live sessie vernieuwt de bronnen vóór projectexport.
  Downloads worden éénmaal per job voorbereid; pas na alle validaties wijzigen
  TIFF-koppelingen. Bestaande geladen objectlijsten en bronbestanden blijven intact.
  Tijdelijke validatie houdt hoogstens één geparseerde objectlijst tegelijk vast.
  Een bekend jobnummer zonder profieldata geeft een gerichte fout; expliciete
  afbeeldingsexport en TIFFs zonder jobidentiteit blijven mogelijk.
- De gemengde sloteditor heeft Ctrl/Shift-selectie, bulk meenemen/uitsluiten,
  PS-nummer/variant, schaal, verplaatsen en lege vakken invoegen/verwijderen.
  Volledige bronidentiteit en oorspronkelijke archiefindices bewegen mee.
- Productieservices verwerken begrensde batches in oorspronkelijke volgorde.
  Maximaal tweemaal het aantal workers ligt vooruit; vroege langzame antwoorden
  vergroten die buffer niet. BGT deelt één HTTP-pool/cache zonder geneste
  vermenigvuldiging van tileworkers. Paden volgen de oorspronkelijke CPU-policy.
  Fouten, panics, quota en annuleren publiceren geen gedeeltelijk request.
- Een handmatige GeoTIFF-download kan vanuit het eigen Edge-venster worden
  vastgelegd. Tab, nonce, job, tijd en berichten zijn begrensd. Fetch/XHR-observatie
  blokkeert het oorspronkelijke antwoord niet. Alleen een opnieuw gevalideerd
  TIFF-voorbeeld promoveert de strategie. Ook mislukte activering ruimt de eigen
  capture op; projectwissels annuleren de taak en verouderde meldingen.

## Bewijs

- **757 Rust-tests over 150 targets slagen**, waaronder 268 kern- en 115
  desktoptests. Het volledige `--all-targets`-log is
  `root-current-viewport-all-targets-reviewed.log`.
- Clippy slaagt voor alle targets, met bestaande/stijlwaarschuwingen:
  `root-current-viewport-clippy.log`. De echte executable en alle acht
  exportdrivers zijn opnieuw gebouwd (`root-current-viewport-production-build.log`).

- Drie originele generators reproduceren de policy-, camera- en slotreferenties
  met identieke genormaliseerde bytes: 175 policies, 40 dekkingsgevallen,
  vijf planningsbeslissingen en 64 echte oorspronkelijke verplaatsingsgevallen.
- Veertien oorspronkelijke Python-tests voor resourcepolicy en kaartaanvragen
  slagen zonder oorspronkelijke appstart. De capture-JavaScriptproef slaagt in
  negen fetch/XHR/GET/budget/niet-blokkeren/identiteits-/opruimscenario's.
- Vijf lokale WebSocketproeven controleren echte CDP-berichten, nonce/job/status,
  annuleren/verlopen, vreemde doelen, oversize/timeout en mislukte activering.
- De Pythonproject → native opslaan/openen → fysieke TIFF-profiel → DXF/Excel-proef
  controleert geschreven profielgeometrie, watertekst en PS42B met één discipline
  en twee objecten. Onbekende velden en oorspronkelijke bronnen blijven intact.

Acht echte exportgroepen slagen via onafhankelijke ezdxf/Pillow-controle:

| Groep | Geschreven uitvoer en inhoudelijke vergelijking |
| --- | --- |
| MTEXT-kolommen | Acht DXFs, 60 oorspronkelijke ouders en 54 herstelde oorspronkelijke metadataverliezen; volledige dictionaryinhoud en zelfstandige eigenaars. |
| XCLIP | Drie DXFs, 30 oorspronkelijke INSERTs en onafhankelijke filter-/attribuutgrafieken. |
| Profielschaal | Vier DXFs, 209 volledige oorspronkelijke profielobjecten; Auto/40/200/5000. |
| Servers | Vijf gepaarde DXFs, 155 volledige oorspronkelijke overzichtsobjecten; fasevolgorde, deduplicatie, union en terreinfallback. |
| Maatvoering | Drie DXFs, vijftig oorspronkelijke DIMENSION-objecten met eigen geometrie, metadata en schone CAD-audits. |
| Werkstromen | Acht echte normale/omgekeerde/afbeelding-/volgorde-/ontbrekende-meting-/punt-/opmerking-/rotatievarianten; profiel, datum/project, beeldpixels en bronbehoud. |
| Voorbereid/CLI | Vijf exports, acht profielen, 26 PNGs; relatieve invoer, volledige CLI-export en expliciete weigering van bestaande CLI-uitvoer. |
| Overzichten | Acht zelfstandig plus acht gepaard; per reeks 91 volledige oorspronkelijke objecten en twaalf affine TIFF-beelden, byte-identieke TIFFs en geen nieuwe auditreparaties. |

Het manifest staat lokaal onder
`high-viewport-exports-16o9e6na/proof.json`; de bronreproductie onder
`high-viewport-source-gvupoe6y/proof.json`. De gecombineerde uitvoer staat in
`root-current-viewport-exports.log`. De echte commandoregelprogramma's worden
uitgevoerd; bestaan van een bestand of succes van een nep-exporter volstaat niet.

## Nog vereist

[De aanvullende bronvergelijking](PARITY_GAPS_2026-10-25.md) beschrijft concrete
resterende instellingen-, browser-, adres-, gemengde-bron- en rasterwerkstromen.
De nieuwe instellingenmodule is nog niet aangesloten of als voltooid meegeteld.
De historische 72-modulegate en huidige inventaris van 76 modules/386 tests
vereisen nog afzonderlijke contractmapping, naast bytecode- en prestatiebewijs.
Echte accounts/providers, gebruikersprojecten, AutoCAD, GPU/vLLM en de volledige
zelfstandige installatie-/update-/rollback-/ondertekeningsproeven blijven open.

De vorige Rust-Windows-run 459 werd na 75 minuten tijdens de thin-LTO-build
afgebroken; Quality Gate 1064 en Windows-build 663 slaagden wel. Die afgebroken
Rust-run is geen geslaagde controle. De oude workflowlimiet was 75 minuten. De workflow behoudt alle controles en de
productie-optimalisatie; de tijdslimiet is verhoogd naar 180 minuten. Een nieuwe
volledige Windows-run blijft vereist.

Cyclomedia en StreetSmart blijven uitgesloten volgens de gebruikersinstructie.
De doelrepository is nog niet gevuld en er is geen definitieve release gepubliceerd.
