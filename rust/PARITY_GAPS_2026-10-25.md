# Aanvullende bronvergelijking, 25 oktober 2026

Dit zijn concrete vervolgtaken uit de oorspronkelijke actieve code en tests.
Deze inventaris vervangt de volledige voorwaarden in [PARITY.md](PARITY.md) niet.
De historische 72 modules en de huidige inventaris van 76 modules met 386
testfuncties vereisen nog een expliciete contractmapping; aantallen geslaagde
Rust-tests bewijzen die dekking niet.

| Werkstroom | Oorspronkelijk contract | Huidige native beperking |
| --- | --- | --- |
| Instellingen toepassen | `settings_navigation_patch._wrap_viewer_save` en `test_settings_navigation.test_viewer_choice_applies_only_after_successful_save`: toepassen na succesvol opslaan. | Hersteld op 26 oktober: aangesloten conceptvenster, gezamenlijk opslaan met rollback/journal en exclusieve lezers-/schrijverslock. Alleen een geslaagde opslag past de actieve waarden toe. Zie HANDOFF_2026-10-26.md. |
| Instellingen bewerken | `settings_navigation_patch._install_navigation`, `settings_ui._open_words_view`, materiaal-/profielkeuze-editors en `marxact_import_patch._edit_mappings`. Tests behouden categorie, waarden en opslagvoet. | Categorieën, woordenlijst, materialen, extra profielkeuzes en naamkoppelingen zijn aangesloten. Een echte egui-weergavetest controleert de opslagvoet op een klein venster. Volledige oorspronkelijke GUI-acceptatie blijft vereist. |
| Browserdownloads | Reguliere browserfallback en capture moeten beide begrensd zijn en annuleren respecteren. | Hersteld in de aanvullende controle van 26 oktober: gedeeld transport met absolute deadlines, annuleerbare tabselectie en socket-I/O, framelimieten en een begrensde browserhelper. Lokale WebSocket- en JavaScript-regressies dekken ook de gewone publieke fallback en navigatierace. Echte accountacceptatie blijft open; zie VERIFICATION_2026-10-26_CONTROLS.md. |
| Gemengde projectdialoog | De oorspronkelijke `_initial_template_export_order_state` begint met alle `self.tiff_layers`. | Hersteld op 26 oktober: niet-gematchte fysieke TIFFs staan naast jobs en virtuele sleuven; identieke gekoppelde bronnen worden niet dubbel toegevoegd. |
| Dubbele fysieke bronpaden | Oorspronkelijke laag-/archiefindices onderscheiden verschillende rijen, ook bij dezelfde bestandsbron. | Hersteld op 26 oktober: fysieke originidentiteit, bronkoppeling en conversie naar Python. Een volledige native/Python-roundtrip toetst dubbele paden, herschikken en onbekende laag-/slotvelden. |
| Sjabloonadres | `CadastralDxfExporter._reverse_geocoded_template_address` vraagt het TIFF-midden op en maakt adresregels. | Aangesloten voor verse export, met expliciete adresvoorrang en best-effort netwerkopvraging. Geïsoleerde oorspronkelijke callbackgevallen en werkelijk geschreven DXF-adressen worden gecontroleerd; echte provideracceptatie blijft open. |
| Exportassets | De actieve `template_export_performance_patch` verdeelt lichte kaarten en zware rasterassets over afzonderlijke begrensde pools. | Aangesloten op 26 oktober: afzonderlijke lichte/zware fasen, begrensde taakverdeling, gedeeld schrijfbudget en veilige workerafronding. Eén versus vier werkers en de vorige/nieuwe executable leveren gelijke DXF-/PNG-uitvoer. De exacte budgetgrenstest vond ook een onderdrukte PNG-afsluitfout; deze is hersteld. Zie VERIFICATION_2026-10-26_ASSETS.md. Grote-projectprestatie en volledige praktijkacceptatie blijven open. |
| Rasterweergave bij uitzoomen | `MapRenderer._paint_axis_aligned_tiff` en `_paint_affine_tiff` gebruiken bilineaire bronfiltering. | Native `raster::sample_positions` neemt eerst losse bronpixels. Latere LINEAR-textuurfiltering bewijst geen identieke verkleining. De decode-oracles gebruiken expliciet NEAREST voor verkleinde referenties. |
| Rastergeheugen | `_prepared_native_tiff_jobs` gebruikt de hardwarepolicy en geeft offscreen caches vrij, inclusief alleen rakende randen. Oorspronkelijke tests controleren beide situaties. | Hersteld op 26 oktober: hardwarequota worden over zichtbare detailplannen verdeeld; offscreen crops verdwijnen. Basispreviews blijven volgens het oorspronkelijke contract behouden. Echte camera-pan/rand/herintrede en achterhaalde resultaten worden getoetst. |

De onderzochte oorspronkelijke renderer selecteert geen TIFF-overviews en
reprojecteert GeoTIFF-wereldcoördinaten niet. Dat is geen bewijs voor ontbrekende
piramide- of CRS-functionaliteit. De first-directory- en affine-contracten,
ondersteunde codecs en uitvoer moeten afzonderlijk bewezen blijven; zie
[VIEWPORT.md](VIEWPORT.md).

Echte accounts en providers, representatieve gebruikersprojecten, AutoCAD,
GPU/vLLM en volledige zelfstandige release-/installatie-/update-/rollbackproeven
blijven eveneens vereist. Cyclomedia en StreetSmart blijven uitgesloten volgens
de gebruikersinstructie; oorspronkelijke archiefgegevens blijven bewaard.
