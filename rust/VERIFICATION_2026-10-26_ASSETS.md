# Exportafbeeldingen en PNG-foutafhandeling, 26 oktober 2026

De sjabloonexport bereidt onafhankelijke afbeeldingen nu parallel voor, met de
bestaande hardwarepolicy en begrensde taakverdeling. Dit sluit de concrete
seriële-assetbeperking uit de aanvullende bronvergelijking.

## Verwerking

- Eerst worden lichte kaarten verwerkt; pas wanneer die werkers zijn afgerond,
  begint de zware fase. Volledige TIFFs, voorbereide PNGs, foto's, PNG-achtergronden,
  kaarten boven 300 DPI en virtuele kaartbronnen boven kwaliteit 1 vallen onder
  het zware werk. Ook voorbereide PNGs worden werkelijk gedecodeerd ter controle.
- `light_map_workers` en `heavy_raster_workers` bepalen de aantallen, met harde
  bovengrenzen van respectievelijk 32 en 4. Ook expliciete omgevingsinstellingen
  kunnen daardoor geen onbeperkte aantallen threads of TIFF-decoders starten.
  De fasen lopen achter elkaar en stapelen hun geheugenpieken niet op.
- Iedere werker hergebruikt zijn eigen kaart-/lettertypecache. Grote bronvectors
  worden geleend; volledige beeldbuffers blijven binnen de actieve taak.
  De geordende scheduler heeft hoogstens tweemaal het aantal werkers vooruit
  ingeplande taken. De resultaten bevatten geen pixels.
- Een gedeeld atomair budget reserveert bytes vóór elke schrijfactie. Het telt
  alle nieuwe afbeeldingen en Normaal/Reverse-kopieën mee binnen het bestaande
  afbeeldingsbudget van 2 GiB, na aftrek van overzichtsafbeeldingen en logo's.
  De bestaande grenzen per bestand en per decode blijven gelden.
- Gelijke kaarten en foto's voor Normaal/Reverse worden eenmaal opgebouwd en
  daarna binnen dezelfde taak gekopieerd. De DXF-handlevolgorde, bestandsnamen,
  profielberekening, beeldposities en publicatietransactie blijven behouden.
- Bij annuleren of een fout stopt de batch zijn eigen werkers en wacht hij tot
  ze afgerond zijn, vóór de tijdelijke exportmap wordt opgeruimd. Een lokale
  assetfout annuleert de aanroeper niet, zodat opnieuw proberen mogelijk blijft.

## Gevonden en herstelde PNG-fout

De exacte budgetgrenstest vond een fout in het bestaande encoderpad:
`image::PngEncoder` laat de laatste PNG-chunk (`IEND`) door de destructor van
`png::Writer` schrijven. Deze destructor kan schrijffouten negeren. Een budget
dat één byte onder de complete PNG lag, kon daardoor toch een succesvolle
export met een afgebroken afbeelding opleveren.

De gecontroleerde schrijver onthoudt nu de eerste schrijf-/flushfout. Na het
afronden van de encoder wordt die fout gecontroleerd voordat een tijdelijk
bestand een zichtbaar exportasset mag worden. De encoder en zijn geslaagde
PNG-bytes zijn gelijk gebleven. De regressie toetst alle twaalf afbreekpunten
in de laatste chunk, zowel voor het gedeelde als het bestaande losse budget.
Een mislukte poging laat geen PNG achter en vervangt geen eerder bestand.
Een afzonderlijke injectietest controleert fouten en schrijfacties van nul bytes
bij het afsluiten. Tijdelijk onderbroken schrijfacties mogen wel worden hervat;
dat pad levert opnieuw de volledige, oorspronkelijke PNG-bytes op.

## Uitvoerbewijs

De nieuwe productietests vergelijken één werker met vier werkers voor voorbereide
PNGs, echte TIFFs, virtuele rasters, referenties, profielen, lege vakken, dubbele
labels, geroteerde foto's en Normaal/Reverse. Ze vergelijken alle PNG-bytes en
de complete DXF, met uitsluitend de eigen bundelpaden en verse AutoCAD-blok-GUIDs
genormaliseerd. Budgetgrenzen, kopieën, annuleren, callbackfouten, behoud van de
vorige publicatie en opnieuw proberen worden afzonderlijk gecontroleerd.

Daarnaast heeft [het CLI-vergelijkingshulpmiddel](../tools/check_template_asset_equivalence.py)
de daadwerkelijke vorige executable van revisie `8f94c84` en de nieuwe executable
uitgevoerd met geïsoleerde instellingen. Alle vier scenario's slagen:

| Scenario | Gelijke PNG-bestanden | DXF na beperkte normalisatie |
| --- | ---: | --- |
| Gemengde voorbereide invoer en foto's | 9 | Gelijk |
| Fysieke TIFF, kaart en Normaal/Reverse | 18 | Gelijk |
| Virtuele sleuf, kaart en Normaal/Reverse | 18 | Gelijk |
| Virtuele sleuf met profielreferentie | 18 | Gelijk |

Zie [de executable-hashes en volledige uitvoerhashes](verification/template-assets-equivalence.json).
De proef gebruikt echte GNU-debugbuilds en synthetische fixtures. De losse
looptijden in het bewijs zijn observaties en onderbouwen geen snelheidsclaim
voor grote gebruikersprojecten.

## Lokale controles

| Controle | Resultaat |
| --- | --- |
| Volledige native testset, alle doelen, vergrendeld/offline | 802 geslaagd over 151 doelen; 0 mislukt |
| Bibliotheek / desktop binnen die set | 295 / 130 geslaagd |
| Clippy, alle doelen | Geslaagd; bestaande waarschuwingen blijven zichtbaar |
| Eén versus vier werkers, echte volledige exports | Vier scenario's met gelijke PNG-/DXF-uitvoer |
| Vorige versus nieuwe executable, echte CLI | Vier scenario's; 63 gelijke PNGs en vier gelijke genormaliseerde DXFs |
| PNG-afsluiting en exacte quota | Beide regressies geslaagd, inclusief twaalf eindchunkgrenzen en geïnjecteerde schrijffouten |

De laatste Clippy-opruiming verandert uitsluitend een equivalente vergelijking
in een testassertie. Beide PNG-regressies en Clippy zijn daarna opnieuw uitgevoerd;
de applicatiecode en de gecontroleerde executable zijn daarbij ongewijzigd.

## Grenzen

Een afzonderlijke medium-review en de integratiereview hebben de fasevolgorde,
geheugenlevensduur, annulering, workerjoins, quota en uitvoercontracten nagelezen.
De vorige Windows-build bij `8f94c84` is volledig groen:
[run 40271779951](https://github.com/LukaStuurman/SleufBase-RUST/actions/runs/40271779951).
Deze oudere build bewijst de nieuwe assetwijzigingen niet.

De complete oorspronkelijke contractmapping, bilineaire viewportverkleining,
echte providers/accounts, representatieve projecten, AutoCAD en zelfstandige
installatie-/update-/rollbackacceptatie blijven onder [PARITY.md](PARITY.md) vallen.
Dit is geen verklaring dat de volledige port klaar of foutloos is.
