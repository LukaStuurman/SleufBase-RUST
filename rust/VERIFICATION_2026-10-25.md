# Integratiecontrole 25 oktober 2026

De volledige acceptatievoorwaarden in [PARITY.md](PARITY.md) blijven open.
Deze controle betreft de huidige geïmplementeerde onderdelen; zij verklaart de
port of de zelfstandige release niet compleet.

- GNU Windows: `cargo test --all-targets --locked --offline --jobs 2
  --manifest-path rust/Cargo.toml` slaagt met **694 tests, nul fouten**, over
  **146 targets**, inclusief voorbeelden zonder eigen tests. Daarvan zijn
  222 kerncontroles en 106 desktopcontroles.
- Dezelfde targets slagen met Clippy, met bestaande en stijlwaarschuwingen.
  De native desktopexecutable en zeven productie-auditdrivers zijn opnieuw
  gebouwd. Dit is een lokale dev-build; Windows/MSVC-releasecontrole blijft
  een afzonderlijke CI-stap.
- Vier oorspronkelijke contractgenerators en één onafhankelijke TIFF-generator
  reproduceren 36 referentiebestanden bytegelijk. De laatste TIFF-controle omvat
  de definitieve 20×24-pixel-4×4-gevallen. Deze generators starten de oorspronkelijke
  app niet en lezen geen oorspronkelijke accounts/gebruikersinstellingen. Echte
  GPU/modellen en modeldownloads zijn niet gebruikt.

De drie medium-agents leverden zes pakketten; de centrale review controleerde
de integratie en voegde bestandsslepen/opstartinvoer en aanvullende correcties toe:

1. Extra TIFF-subsampling ondersteunt strips/tegels, raw/Deflate en bestaande
   ModernJPEG, met dezelfde bronpixels voor zoomuitsneden en volledige export.
   Dertig onafhankelijke bestanden toetsen bytevolgorde, BigTIFF, oriëntatie en
   opgevulde randen. Niet-JPEG is pixelgelijk; de gedocumenteerde JPEG-codecgrens
   blijft twee kleurwaarden, of drie voor de 2×2-tegelvariant. Ontbrekende/dubbele
   directorytags en te grote gecomprimeerde segmenten zijn afgeschermd. Zie
   [de TIFF-oracles](tests/fixtures/subsampled-tiff/README.md).
2. Automatische AHN-aanvragen hervatten bij projectopen/import en gebruiken één
   begrensde worker. Project-/puntwijzigingen, handmatige/gemeten hoogtes en
   annulering blijven beschermd. Tienduizend pending-objecten delen één
   bronidentiteit; naam en bronpad worden niet per punt gekopieerd. De oorspronkelijke
   blanket-overschrijving van objecthoogtes wordt bewust niet herhaald.
3. KickTheMap-joblijst en cache hebben een vaste, bewezen account- of
   browsersessie-eigenaar. Browserovername en andere accountacties zijn onderling
   afgeschermd; een late download kan geen andere sessie terugzetten. Een gewijzigd
   e-mailveld verandert de eigenaar niet. SHA-256 met de reeds gebundelde
   TLS-cryptobibliotheek houdt ook voor geldige 254-byte-e-mailadressen de
   cachenaam op 77 tekens. Onbekende browseridentiteit krijgt een aparte sessiesnapshot.
4. Het hersteloverzicht scant native en oorspronkelijke autosaves uitsluitend
   leesbaar op de achtergrond. Inhoud bepaalt het formaat; herstel gebruikt de
   normale annuleerbare loader, wijzigingsbevestiging en **Opslaan als**. Een
   mislukte herstelpoging wist geen actief kaartconcept. De datumkolom is expliciet UTC.
5. XCLIP-INSERT-kopieën behouden hun eigen filter-/dictionary-/attribuutgrafiek,
   geometrie, matrices en metadata, met gedeelde bronblokken. Echte oorspronkelijke
   ACAD_TABLE-/proxy-kopieën zijn in de referentie als `CopyNotSupported` vastgelegd.
6. Projectvakken bewaren een eigen automatische/handmatige profielschaal en
   nemen oorspronkelijke bronmetadata over. De oorspronkelijke schaalparser en
   metadata-coercion zijn vergeleken; de beperkte Python-terugconverter neemt de
   schaal over zonder onbekende velden weg te laten.

Verkenner-/opstartpaden en echte egui-dropgebeurtenissen delen één TIFF/DXF-batch.
Projectvoorrang, onopgeslagen werk, gedeeltelijke bestandsfouten, duplicaten,
annulering vóór publicatie en uitgestelde invoer bij projectwissels zijn getoetst.
Een Python-autosave met `.json` wordt inhoudelijk herkend en nooit een native
opslagdoel. Windows-opstartpaden blijven `OsString`/`PathBuf`.

Zeven groepen daadwerkelijk opnieuw gebouwde productie-uitvoer slagen:

| Groep | Onafhankelijk bewijs |
| --- | --- |
| XCLIP | Drie native DXFs, dertig oorspronkelijke INSERT-kopieën, zelfstandige eigenaarsgrafieken, gedeeld bronblok en fout-/reparatievrije CAD-audits |
| Profielschaal | Vier projectplanner-DXFs, 209 volledige oorspronkelijke profielobjecten, Auto/1:40/1:200/1:5000 en baseline-relatieve CAD-audits |
| Verse sjabloondiensten | Vijf gepaarde DXFs, 137 oorspronkelijke overzichtsobjecten, dienstvolgorde, deduplicatie, oppervlak-union en terrein-fallback |
| Maatvoering | Drie DXFs, vijftig oorspronkelijke maatvoeringen, zelfstandige geometrie-/metadatagrafieken en schone CAD-audits |
| Gewone workers | Acht exports: normaal/reverse, alleen beeld, geordende varianten, ontbrekende metingen, gekozen meetpunten, TIFF-opmerkingen en rotatie; datum/projectnaam/Excel/pixels en bronteruggave |
| Voorbereide uitvoer | Vijf exports, acht volledige profielen en 26 blijvende PNGs; volgorde/velden/eigendom/pixels/bronbehoud. De echte CLI toetst relatieve manifestpaden, automatisch Excel, licenties en weigering van overschrijven |
| Kadastraal overzicht | Acht zelfstandige en acht gepaarde exports, elk 91 complete oorspronkelijke overzichtsobjecten en twaalf affine IMAGEs; bytegelijke TIFFs, kleuren/stijlen, tekenvolgorde, profielsplitsing en wederzijdse reactors |

De review herstelde ook de controles zelf: ezdxf maakt bij het lezen ontbrekende
SEQEND-objecten met nieuwe handles. Die niet-geserialiseerde objecten mogen geen
nieuwe native profielobjecten uitsluiten. De schaalcontrole gebruikt daarom
oorspronkelijke modelruimte-entiteiten; de voorbereide uitvoercontrole gebruikt
uitsluitend werkelijk aanwezige bronhandles. Bewust gewijzigde maaiveldtekst en
een geometrieverplaatsing van één centimeter worden door de volledige schaaloracle
afgekeurd. De controle op corrupte XCLIP-eigenaars wijzigt de echte eigenaar,
buiten APPDATA-reactorverwijzingen, en bewijst behoud van de oorspronkelijke DXF.

Lokale logs staan onder `%LOCALAPPDATA%\SleufBaseDev`:
`root-high-review-next-final-all-tests.log`, `root-high-review-next-final-build.log`,
`root-high-review-next-final-clippy.log`, `root-next-original-proof.json` en
`root-high-reviewed-exports-proof.json`.

Praktijkacceptatie met representatieve projecten/accounts, echte diensten/GPU,
resterende oorspronkelijke functies, de 72 Python-regressiecontracten en de
zelfstandige overdracht/installatie/update/rollback/signing blijven afzonderlijk
vereist. De port blijft een concept; de definitieve doelrepository wordt pas
na volledige verificatie gevuld.
