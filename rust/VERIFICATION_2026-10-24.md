# Integratiecontrole 24 oktober 2026

Deze controle geldt voor de geïmplementeerde onderdelen. De volledige
acceptatievoorwaarden in [PARITY.md](PARITY.md) blijven open totdat zij elk met
passend bewijs zijn afgedekt; dit rapport verklaart de port niet compleet.

- GNU Windows: `cargo test --all-targets --locked --offline --jobs 2` met
  `--manifest-path rust/Cargo.toml`: **665 tests geslaagd, nul fouten**, verdeeld
  over 142 targets inclusief de voorbeeldprogramma's zonder eigen tests.
- De afsluitende desktopcontrole slaagt opnieuw met 98 tests na de cachebewaking.
- Dezelfde targets zijn met Clippy gecontroleerd. De controle voltooit met
  stijlwaarschuwingen; er is geen claim van een waarschuwingvrije codebasis.
- Vier oorspronkelijke generators produceren 16 verse bestanden bytegelijk
  aan de AI-, profiel-, overzichts- en virtuele-kaartreferenties. App, accounts,
  oorspronkelijke instellingen, GPU en modeldownloads worden niet gestart.
- Acht echte zelfstandige overzichtsexports en acht gepaarde sjabloonexports
  vergelijken elk 91 volledige oorspronkelijke overzichtsobjecten en twaalf
  affine IMAGEs met bytegelijke TIFF-kopieën. Vergelijking omvat tekenvolgorde,
  laagkleuren/lijndiktes, effectieve font-/stijlwaarden, beeldreactors, scheiding
  van profielstaten en afwezigheid van nieuwe DXF-auditreparaties.
- Acht bestaande productie-workerexports slagen opnieuw: normaal, reverse,
  alleen beeld, geordende varianten, ontbrekende metingen, gekozen meetpunten,
  TIFF-opmerkingen en rotatie. Geschreven profielen en TIFF-pixels worden met
  de oorspronkelijke route vergeleken; bronallocaties keren terug.
- Vijf voorbereide exports leveren acht volledige profielen en 26 blijvende
  PNGs. De echte executable slaagt met manifestrelatieve invoerpaden, automatisch
  discipline-Excel en weigering om bestaande uitvoer te overschrijven. De
  broncontrole accepteert precies het oorspronkelijke contactherstel; een
  bewust gewijzigde andere sjabloontekst wordt afgekeurd.

Vijf nieuwe echte gepaarde sjabloonexports vergelijken 137 volledige oorspronkelijke
overzichtsobjecten, dienstvolgorde, oppervlakten en de terrein-fallback. Drie extra
DXFs vergelijken 50 maatvoeringen met eigen geometrieblokken, exacte XDATA/XRECORD-
waarden, ongewijzigde named resources en een fout-/reparatievrije CAD-audit.
Veranderde maatvoeringstekst en een verschoven primitief van één centimeter worden
bewust afgekeurd. De oorspronkelijke dienst-, maatvoering- en projectgenerators
produceren vier nieuwe referentiebestanden opnieuw bytegelijk.

De aanvullende integratieronde omvat ook:

- Eigen Windows-modelprocessen starten gesuspendeerd en krijgen vóór uitvoering
  een niet-erfelijk Job Object met kill-on-close. Native kind-/kleinkindproeven
  toetsen startup-annulering, een al gestopte ouder, mislukte afscherming en het
  behoud van een extern proces. Echte GPU-/vLLM-acceptatie blijft afzonderlijk.
- Camera-uitsneden vergelijken volledige bronpixels voor acht TIFF-oriëntaties,
  alpha/palet/planar en minder gedecodeerde stroken. De echte kaart verdeelt basis
  en detail over disjuncte affine vlakken, zodat dekking niet dubbel wordt
  toegepast. Bron-/generatiebewaking, hysteresis en een gezamenlijk geschat
  RGBA-textuurbudget van 256 MiB zijn aangesloten.
- Vrije kaartopmerkingen hebben klikplaatsing, multiline-bewerking en slepen met
  preview. Werkelijke egui-gebeurtenissen toetsen commit, Escape, loslaten buiten
  de kaart, camera-/projectbehoud en afwijzing van achterhaalde of ongeldige edits.
  42 oorspronkelijke hitgevallen zijn opnieuw bytegelijk gereproduceerd.
- Python-projectexport gebruikt het oorspronkelijke contract voor 18 instellingen,
  slots, schalen en PS-varianten. Onbekende velden blijven behouden; uitsluitend
  werkelijk niet door Python toepasbare keuzes geven een gerichte fout.
- KickTheMap heeft sessiegebonden downloadidentiteiten voor prefixloze projecten,
  bevestiging van onverwacht lege lijsten vóór cachevervanging en behoud bij
  fouten. De echte browserfallback behoudt dezelfde ruwe identiteit en guard;
  lokale HTTP- en transportproeven toetsen dit zonder accountaanmelding.
- Verse sjabloondiensten volgen de oorspronkelijke lokale gebiedsberekening,
  inclusief 15-naar-12 samenvoegingen en gebieden breder dan de interactieve
  2 km-download. Overzicht/profielcontext en geselecteerde CAD-kaarten volgen
  hun afzonderlijke oorspronkelijke route. Offline onderdrukt ook luchtfoto's.

Centrale review herstelde onder meer het starten van een tweede modelproces na
een tijdelijke gereedheidstime-out, behoud van oude AHN-modetekst, een AI-preview
bij een andere bronkeuze, dubbele metadataquotering, onbegrensd TIFF-kopiëren,
niet-herhaalbare DXF-klassevolgorde en afwijkende nieuw aangemaakte overzichtslagen.
Annulering, bronbescherming, terugkeer van eigendom en strikte projectinstellingen
hebben gerichte regressieproeven.

De twee afsluitende spaties in `cadastral_overview_base.txt` zijn oorspronkelijke
DXF-lijntypebeschrijvingen. Deze gegevens blijven bytegetrouw, net als de DXF-
referenties; zij worden uitgezonderd van de broncodecontrole op witruimte.

Echte GPU-/vLLM-voorspellingen en procesacceptatie met werkelijke modelworkers,
dienst-/accountvarianten, representatieve projecten, AutoCAD-acceptatie,
resterende kaartbedieningen, volledige Python-terugconversie, prestatieacceptatie
en installatie/update/rollback blijven afzonderlijk te bewijzen. De zelfstandige
overdracht naar `LukaStuurman/SleufBase-RUST` volgt pas na de volledige port en
alle leveringsvoorwaarden.
