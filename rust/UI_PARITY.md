# UI-pariteit SleufBase: open acceptatielijst

Status: het volledige UI-doel is **open**. De oorspronkelijke hoofd-shell, het thema, de inline locatiezoeker, handmatige maaiveldinvoer en instellingentabs zijn hersteld als geteste tussenstap. De hoofd-shell is gecontroleerd met widgetinspectie en egui-PNGs. Er is geen Tk/egui-pixeldiff uitgevoerd; dat bewijs geldt niet als voltooiing van alle vensters, menus en instellingen.

Deze inventaris is een statische audit van bronbestanden en codeobjecten. Hiervoor zijn geen app, accounts, gebruikersinstellingen of bestaande gebruikerssessies geopend. Er zijn geen Cargo- of Git-opdrachten uitgevoerd.

## Update van de huidige UI-branch (26 oktober 2026)

De vergelijkende tabel hieronder beschrijft de **oude uitgangssituatie op `main`**;
zij is niet automatisch de toestand van de open PR. In
[PR #1](https://github.com/LukaStuurman/SleufBase-RUST/pull/1)
zijn inmiddels de volgende UI-oppervlakken aangepast:

- KickTheMap Jobs: zeven kolommen met aparte filters, Jobs-kaart en Open job
  in het eigen Edge-profiel; echte accountacceptatie nog niet voltooid.
- Fysieke TIFF-lijst: afzonderlijk rechtermuisknopmenu met rotatie,
  commentaar en een bewerkbare virtuele geometriekopie.
  Dit is **geen kopie van de rasterpixels**.
- Virtuele TIFF-lijst: naam, verplaatsen, 90°/handmatige/interactieve rotatie,
  reset, virtuele kopie, puntbewerking en opmerkingen. Geometrische validatie
  blijft leidend bij gemeten sleuven.
- Punt- en dekbandvensters: aparte kettingmaatstappen en dekbandbeheer met
  begindiepte, einddiepte en annuleren.
- marXact: dialoog 'MarXact kabel-/leidingoriëntatie' met bronkeuze,
  correctie van -90° tot +90° en automatische herberekening.
- DXF-export: titel en bronkolommen van het origineel; geselecteerde
  geauthenticeerde live jobs zijn vanuit hetzelfde venster te downloaden,
  komen daarna in de bronlijst en worden automatisch aangevinkt.
- CAD-overlap: popup 'Gevonden lagen (N)' om een van meerdere treffers te kiezen.
- Aanvullende UI-regressies: na annuleren van virtuele rotatie komt de muiscapture
  vrij; een geweigerde rotatie onderbreekt geen andere kaartbewerking; gekoppelde
  live-DXF-selecties hebben een consequent alles-selecteren/wissen-gedrag.

**Bewijslimiet:** De eerdere Jobs-baseline heeft een groene Windows-run.
Voor de nieuwste branch moeten alle Cargo-tests, Clippy, releasebuild en
portable packaging slagen. Daardoor zijn de nieuwe interacties nog niet
automatisch volledig geaccepteerd. Er is geen visuele Tk-versus-egui-pixeldiff
en geen eind-tot-eindtest met een echte KickTheMap-account. De oorspronkelijke
aparte Toplevel-vensters, de volledige instellingsvelden, oorspronkelijke
tooltip-/focusvolgorde en overige context-/exportdetails blijven open.

## Bewijs en scope

- Native werkboom: deze repository, `LukaStuurman/SleufBase-RUST`.
- Oorspronkelijke referentie: `LukaStuurman/SleufBase`, met de hieronder vermelde bronhashes.
- `_bytecode/app.cpython-311.pyc`: grootte 731007 bytes en SHA-256 `723928f95127363585753065d5440c4e3be242089d590d58029a2214f0caebec` zijn in deze audit gecontroleerd tegen `_bytecode/legacy_bytecode_manifest.json`. Alleen `marshal`/codeobjectinspectie; het app-codeobject is niet uitgevoerd.
- Hieronder betekent `app.pyc:functie:regel` de oorspronkelijke `co_firstlineno` uit dat gevalideerde codeobject. Dit zijn geen regelnummers van de huidige wrapper `app.py`.
- Actieve patches gaan voor de basis-bytecode. Wrapperinstallatie staat in oorspronkelijke `app.py:3232-3244`; de instellingen- en marXact-hooks in oorspronkelijke `__init__.py:40-56`.
- Cyclomedia, StreetSmart en de Gemma-AI-functie zijn op gebruikersverzoek vervallen. Hun vensters en bediening zijn geen acceptatievoorwaarde. Handmatige maaiveldinvoer en AHN-aanvulling blijven binnen scope.
- De genoemde native regels zijn een momentopname tijdens lopend rootwerk. Wijzigingen moeten opnieuw aan deze lijst worden getoetst.

## Grootste concrete verschillen

| Prioriteit | Zichtbaar verschil | Oorspronkelijk bewijs | Native momentopname / acceptatie |
| --- | --- | --- | --- |
| P1 | Instellingen: tabvolgorde en kaartkeuzes gedeeltelijk hersteld | `settings_navigation_patch.py:9-15`: **Kaarten, DXF-export, Proefsleuven, KickTheMap, Back-ups**. `:50-67`: **Achtergrond op het kaartscherm**. `app.pyc:open_settings_dialog:3491`: exportachtergrond | Tabs en beide achtergrondkeuzes zijn hersteld, met een aparte projectgebonden kaartkeuze die pas na succesvolle opslag wordt toegepast. De globale startvoorkeur voor de kaartschermbron wordt nog niet los van een project bewaard. Volledige veld-/layoutvergelijking blijft open. Cyclomedia-optie uitgezonderd. |
| P1 | Instellingen gebruiken nog een intern venster | `settings_maximized_window_patch.py:14-53`: gemaximaliseerde Toplevel, minimum 860x620 (begrensd door scherm), fullscreen fallback | De titel **Instellingen** en ruime viewportvullende indeling zijn hersteld; footer en vijf tabs zijn in widgettests en CPU-beelden gecontroleerd. Het blijft een intern egui-venster; de oorspronkelijke aparte gemaximaliseerde Toplevel en volledige lay-out zijn nog niet gereproduceerd. |
| P1 | Jobs-browser en aanmelding zijn een andere gebruikersroute | `app.py:200-260`: **Jobs** opent aparte Jobs-browser; `kickthemap_jobs_browser.py:28-29,44-52,842,919-975`: 1280x760; zeven kolommen **Jobnaam, Adres, Coordinaten, Aanmaak datum, Status, Gemeente, Job ID**, elk eigen filter; **Kaart**, **Download herstellen**, **Laad GeoTIFFs**, **Open job** | `ui_shell.rs:109` stuurt Jobs en Inloggen naar hetzelfde scherm. `ui_kickthemap_browser.rs:320-430`: 850x670 intern venster, Edge/sessiebediening, enkele zoekregel en tekstregels. Kolomfilters, aparte geïntegreerde jobroute en jobs-kaart zijn niet zichtbaar in dit adaptervenster. Bewust gewijzigde aanmeldtechniek ontslaat niet van beoordeling van zichtbare route en bediening. |
| P1 | Fysieke en virtuele TIFF-contextmenus hebben andere acties/structuur | `app.pyc:_create_tiff_context_menu:3408`: fysieke en virtuele menus afzonderlijk; fysieke **Maak virtuele kopie...**; virtuele **Naam wijzigen...**, **Draai 90 graden linksom/rechtsom**, **Hoek invullen...**, **Virtuele kopie plaatsen...**, **Punt op kaart toevoegen...** | `ui_shell.rs:205-209` combineert diverse controls op de hele kaart. `ui_raster_rotation.rs:96-153` heeft inklapbaar **TIFF draaien**; `ui_virtual_map.rs:551-595` heeft andere knoppen en geen direct equivalente virtuele rotatie-/naamacties. Vergelijk per geselecteerd laagtype inclusief lege lijst, rechtsklikselectie, menuvolgorde en sluiten na actie. |
| P1 | Handmatige punt-/dekbandvensters zijn samengevoegd of ontbreken als oorspronkelijke stappen | `app.pyc:_open_template_dekband_dialog:2006` 800x600; `_open_virtual_trench_dekband_entry_dialog:2154` 860x680; `_open_virtual_trench_point_chainage_dialog:2359` 600x320, minimum 560x280; `_open_virtual_trench_dekband_manager:2461` 760x420; `_open_virtual_trench_point_dialog:8426` | `ui_trench_editor.rs:244` heeft gecombineerd venster 730x650 en dekbandsectie rond `:333`; `ui_virtual_map.rs:566-595` gebruikt prikmodus/inlinevelden. Behoud oorspronkelijke afzonderlijke workflows: afstand -> Verder -> puntdetails, eindpuntbeperkingen, dekbandmanager, gelijke einddiepte en annuleren. |
| P1 | marXact-orientatiedialoog heeft geen zichtbare equivalente adapter | `marxact_alignment_ui_patch.py:91-95`: **MarXact kabel-/leidingoriëntatie**, 650x310, minimum 590x290; `:125-155` sleufkeuze en rotatiecorrectie -90..90; menu-injectie `:331` | `ui_shell.rs:87,112` biedt alleen marXact-import. Geen overeenkomstige titel/rotatiecorrectie gevonden in `ui_trench_editor.rs`. Accepteer pas na bediening voor selecteren, live correctie en terugzetten/sluiten. |
| P2 | Object-DXF-export toont andere bronnen en informatie | `app.pyc:open_kickthemap_dxf_export_dialog:5196`: **KickTheMap DXF-export**, 1160x700; kolommen **Bron, Type, Datum, Details, Status**; live jobs + virtuele bronnen; **Vernieuwen**, **Exporteer geselecteerde bron(nen)...**, **Exporteer + laad als KLIC-overlay** | `export_dialog.rs:54-167`: **Objecten naar DXF exporteren**, breedte 740; uitsluitend geladen datasets/virtuele sleuven en aanvullende technische opties. `ui.rs:974-978` opent direct dit scherm. Beoordeel live bronselectie, datum/details/status en export+overlay-route. |
| P2 | Sjabloonexport heeft twee verschillende native schermen | Actieve vervanger `app.py:2080-2471`: **DXF-sjabloon exporteren**, niet herschaalbaar, positie hoofdvenster +120/+80; lijst links, bewerkingen rechts, projectnaam onder lijst, Annuleren/Exporteren onderaan | `template_dialog.rs:487-496` is herschaalbaar/breedte 900. `ui_exporting.rs:35-38` routeert sommige projecten naar `ui_project_template.rs:99-185`: 730x650 met andere titel en technische stapvelden. Oorspronkelijke gebruikersroute, labels en bereikbaarheid moeten ook voor fysieke, virtuele en gemengde projecten gelijkwaardig worden beoordeeld. |
| P2 | Kaart- en kadastrale export voegen andere configuratiestappen toe | `app.pyc:export_maps:5987` kiest direct exportmap; `export_cadastral_dxf:6020` kiest direct uitvoer-DXF; configuratie staat in Instellingen | `ui_map_export.rs:55-81` en `ui_cadastral_overview.rs:84-94` openen extra instellingenvensters. Beoordeel oorspronkelijke route, standaardwaarden, annuleren en plaats van instellingen. |
| P2 | Back-ups/woordenlijst: vensters nog vergelijken | `settings_kickthemap_autosave_patch.py:245-257`: **Beschikbaar: ...**, **Autosave laden…** in Back-ups. `autosave_restore_ui.py:124-125`: **Autosave laden**, 760x430. `settings_ui.py:607-615`: apart **KickTheMap – woordenlijst** | **Back-ups**, beschikbaar-aantal, **Autosave laden…** en de woordenlijstroutetitel zijn hersteld. De inventarisatie loopt buiten de UI-thread; annuleren en mislukte opslag behouden de kaart. `recovery_ui.rs:43` heet nog **Autosave herstellen**; aparte vensters en volledige woordenlijstindeling moeten nog worden vergeleken. |
| P2 | Kabel-/laaginformatie heeft geen oorspronkelijke popuproute | `app.pyc:_show_feature_popup:11413`: topmost popup **Gevonden lagen (...)** naast de kaartselectie | `ui_shell.rs:163-169` toont gegevens in vaste Kabelinformatie-kaart. Vergelijk meerdere overlappende features/lagen, kiezen en wegklikken; een vaste detailkaart alleen bewijst de popup niet. |

## Volledige acceptatielijst van oorspronkelijke oppervlakken

Alle regels blijven open totdat bronvergelijking en zichtbare bediening zijn gecontroleerd. Bestaande native adapters zijn alleen vindplaatsen, geen pariteitsbewijs.

### Hoofdvenster en menus

- [ ] Hoofdvenster **SleufBase**: oorspronkelijk 1500x930, minimum 1200x760 (`app.pyc:__init__:260`, branding `app.py:267`); sidebar Weergave, GeoTIFF Proefsleuven, KLIC Overlays, Kabelinformatie; kaart, status en RD-coordinaten (`_build_layout:417`). Hier is widget-/PNG-bewijs, nog geen Tk-pixeldiff.
- [ ] Inline **Locatie** met suggesties, toetsenbord, focus en kaartmarkering (`app.py:425-881` vervangt `open_location_dialog:3475`).
- [ ] **Acties**: Project laden..., Project opslaan, Project opslaan als..., Laad GeoTIFF(s), Laad KLIC DXF, Zoom naar alles, Toon TIFF-adressen, Export kaarten, Kadastrale export DXF, Proefsleuven sjabloon DXF, Wis alle referentiepunten, Laad alle beginpunten, Wis opmerkingen, Wis alles (`_build_menu:2886`; latere actieve patches meenemen).
- [ ] **KickTheMap**: Jobs, Inloggen..., Exporteer objectpunten naar DXF..., Uitloggen; de actieve patch verwijdert Browser/Laad GeoTIFF uit job en zet Jobs bovenaan (`app.py:192-263`).
- [ ] **Instellingen**, marXact-import en orientatiemenu (`marxact_alignment_ui_patch.py:331`).
- [ ] Fysieke TIFF-contextmenu: Draai 90° linksom/rechtsom, Interactief draaien, Hoek invullen..., Reset rotatie, Maak virtuele kopie..., Opmerking bewerken..., Opmerking wissen (`_create_tiff_context_menu:3408`).
- [ ] Virtuele TIFF-contextmenu: Virtuele proefsleuf bewerken..., Naam wijzigen..., Verplaats op kaart..., Draai 90 graden linksom/rechtsom, Interactief draaien, Hoek invullen..., Reset rotatie, Virtuele kopie plaatsen..., Punt op kaart toevoegen..., Opmerking bewerken..., Opmerking wissen. Ook kaartcontextmenus voor verplaatsen/draaien/bewerken en lege kaart **Virtuele proefsleuf toevoegen** (`_create_tiff_context_menu:3408`).
- [ ] Lijstselectie, rechtsklikselectie, Delete/BackSpace, KLIC-zichtbaarheid en verwijderknoppen (`_build_layout:417`).

### Vensters, prompts en exports

- [ ] Algemene enkelregelige prompt (`_prompt_single_line_input:876`): o.a. naam/hoek; multiline prompt (`_prompt_multiline_input:975`): opmerkingen. Controleer titel, beginwaarde, opslaan/annuleren, Enter/Escape en focus.
- [ ] Virtuele proefsleuf maken/bewerken (`_open_virtual_trench_editor:9357`); meetobject/eindpunt (`_open_virtual_trench_point_dialog:8426`); punt handmatig op afstand (`:2359`). Native vindplaatsen `ui.rs:1320`, `ui_trench_editor.rs:244`, `ui_virtual_map.rs:551`.
- [ ] **Dekband toevoegen** (`:2006`), **Dekband invoeren** (`:2154`), **Dekbanden** (`:2461`).
- [ ] **Maaiveld invullen - ...** (`_open_maaiveld_marker_dialog:2739`): 560x340, locatie, Maaiveldtekst, Lijnkleur, Kies kleur, Herstel standaard, Annuleren, Opslaan. In uitvoering in `ui_maaiveld.rs:232`.
- [ ] **KickTheMap inloggen** (`open_kickthemap_login_dialog:4814`): accountkeuze, verwijderen account, wachtwoordveld, annuleren/inloggen en fout/status. Geen gebruikersaccount openen voor bronaudit.
- [ ] **KickTheMap jobs**: actieve aparte 1280x760 browser, per-kolomfilters, multi-selectie, Kaart en geïntegreerde Open job (`kickthemap_jobs_browser.py:842-975`). De bytecodebasis `open_kickthemap_jobs_dialog:4970` (1120x620) is hier door de actieve Jobs-launcher voorbijgestreefd.
- [ ] **KickTheMap DXF-export** (`:5196`), **DXF-sjabloon exporteren** (actieve `app.py:2080`), kaartmapkeuze (`export_maps:5987`) en kadastrale uitvoerkeuze (`export_cadastral_dxf:6020`). Controleer voortgang, annuleren, fouten, gedeeltelijke resultaten en afrondmelding.
- [ ] **TIFF-adressen** (`_open_tiff_addresses_dialog:11309`): 980x430, Afbeelding/Adres/Afstand, selectie, Kopieer adres, Kopieer met bestandsnaam, dubbelklik, Sluiten. Adapter `ui_tiff_addresses.rs:117` bestaat; pariteit nog toetsen.
- [ ] **Gevonden lagen (...)** (`_show_feature_popup:11413`) en kabeldetails bij kaartselectie.
- [ ] **MarXact kabel-/leidingoriëntatie** (`marxact_alignment_ui_patch.py:91`), 650x310.
- [ ] **Autosave laden** (`autosave_restore_ui.py:124`), 760x430; projectladen/-opslaan/-opslaan als en bevestigingen rond niet-opgeslagen werk.

### Instellingen

- [ ] **Instellingen** gemaximaliseerd; tabs in volgorde **Kaarten / DXF-export / Proefsleuven / KickTheMap / Back-ups**, context-help en blijvend bereikbare Annuleren/Opslaan (`settings_navigation_patch.py:9-15,103-161`, `settings_maximized_window_patch.py:14`).
- [ ] **Kaarten**: kaartschermachtergrond, exportachtergrond, achtergrond zichtbaar, KLIC-overlays in kaarten/sjablonen; uitzonderingen Cyclomedia/StreetSmart weglaten.
- [ ] **DXF-export**: proefsleufweergave, TIFF-raster meeleveren, PS-tekstafstand, hartlijn- en PS-tekstkleur (`app.pyc:open_settings_dialog:3491`; zichtbare tekstvervangingen `settings_ui.py:24-47`).
- [ ] **Proefsleuven**: automatische dwarsprofielen, reverse-versie, 0-punt/referentie, kaartpunten maaiveld, multileader-overlap, clipping, diameter, opdrachtgeverlogo, BGT-maaiveld en discipline-Excel (`app.py:2612`; `settings_ui.py:24-47`; basisdialog `:3491`).
- [ ] **KickTheMap**: materiaalkeuzes, extra Kabel/Leiding-keuzes, marXact-naamkoppelingen en aparte **KickTheMap – woordenlijst** met woorden/codes, DXF-laag, kleur en profielnaam (`settings_navigation_patch.py:145-161`, `settings_ui.py:607`, `settings_kickthemap_autosave_patch.py`).
- [ ] **Back-ups**: automatisch opslaan, interval, maximum, beschikbaar aantal, **Autosave laden…** (`settings_kickthemap_autosave_patch.py:190-257`).

## Nog vereist voor afsluiting

Per oppervlak: oorspronkelijke en native starttoestand vergelijken, labels/volgorde/geometrie controleren, hoofdactie en annuleren bedienen, toetsenbord/focus/scroll testen en resultaat plus foutpad controleren. Pas daarna afvinken. De hoofd-shell alleen, succesvolle exports of bestaande adapterfuncties zijn geen bewijs dat het volledige UI-doel is afgerond.
