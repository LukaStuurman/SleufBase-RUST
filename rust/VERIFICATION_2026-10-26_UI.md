# Oorspronkelijke SleufBase-UI: geteste tussenstap

Het volledige doel blijft open: dezelfde UI als de normale SleufBase, met de
eerder gevraagde uitzonderingen Cyclomedia en StreetSmart. De resterende vensters,
contextmenu's en acceptatievoorwaarden staan in [UI_PARITY.md](UI_PARITY.md).

## Aanpassingen

- De menubalk volgt de actieve oorspronkelijke launcher: Locatie, Acties,
  KickTheMap, AI, Instellingen en marXact. Instellingen opent direct. Project laden
  herkent zowel native projecten als oorspronkelijke Python-projecten, ook JSON.
- De vier zijpanelen, kaartkaart, drie kaartknoppen, status links en RD-coördinaten
  rechts vervangen de brede migratieknoppenbalk. Andere bestaande hulpmiddelen
  blijven bereikbaar via rechtsklikken op Instellingen of Ctrl+Shift+U.
- Oorspronkelijke startmaat 1500×930, minimum 1200×760, icoon, Windows Segoe UI,
  kleuren en afgeronde panelen zijn teruggebracht. Lege lijstpanelen behouden hun
  hoogte en hebben vaste schuifbalken. Verticale paneelverdeling volgt de gewichten.
- Locatie zoeken staat onderin de kaart, met suggesties, markeren, toetsenbord en
  sluiten. Handmatige maaiveldinvoer gebruikt begin/midden/eind van het profiel,
  tekst, kleur, standaardherstel en veilige concepten. Escape in het tekstvenster
  verwerpt het concept en behoudt de markerstand.
- Kabelselectie gebruikt dezelfde getekende CAD-paden als de kaart, met clipping,
  laagzichtbaarheid, bloktransformaties en de oorspronkelijke selectietolerantie.
- Instellingen heeft weer Kaarten, DXF-export, Proefsleuven, KickTheMap en Back-ups.
  Kaartscherm- en exportachtergrond zijn bereikbaar. De back-uplijst wordt buiten
  de UI-thread geïnventariseerd. Alleen succesvolle opslag past kaartkeuzes toe;
  een ouder concept kan een inmiddels hersteld project niet wijzigen.
- UI-tests lezen voortaan geen persoonlijke instellingen of back-ups.

## Bewijs

`tests/fixtures/ui/original-shell.json` legt 44 oorspronkelijke widgets, 112
widget-/layoutcalls, 46 ttk-stijlconfiguraties en 13 bronhashes vast. Individuele
gevalideerde codeobjecten zijn tegen nepwidgets uitgevoerd; de oorspronkelijke
app, constructor, gebruikersinstellingen en accounts zijn niet gestart/geopend.
`tools/check_ui_contract_sources.py` controleert de bronhashes zonder appcode uit
te voeren.

De laatste volledige controle: **828 tests geslaagd over
151 targets**, inclusief **156 desktoptests**. Clippy
slaagt met waarschuwingen; deze wijziging claimt geen waarschuwingsvrije code.
Zie [het machineleesbare bewijs](verification/ui-shell-verification.json).

De beelden komen uit de echte egui-meshes via een CPU-renderer, na het stabiliseren
van fonts en vensters. Ze zijn visueel bekeken op beide venstermaten en voor de
instellingentabs Kaarten/Back-ups. Het zijn geen GPU-schermopnames en er is geen
Tk/egui-pixeldiff uitgevoerd. Ze bewijzen dus geen volledige visuele gelijkheid.

- [Hoofdvenster 1500×930](verification/ui-shell/shell-1500x930.png)
- [Hoofdvenster 1200×760](verification/ui-shell/shell-1200x760.png)
- [Instellingen: Kaarten](verification/ui-shell/settings-maps.png)
- [Instellingen: Back-ups](verification/ui-shell/settings-backups.png)

## Nog open

Onder meer de oorspronkelijke Jobs-/loginroute, contextmenu's, punt-/dekband- en
exportvensters en marXact-oriëntatiedialoog moeten verder worden overgenomen en
visueel gecontroleerd. Instellingen is nog een intern egui-venster, geen aparte
gemaximaliseerde Toplevel. De kaartschermkeuze wordt met het project bewaard;
een afzonderlijke globale startvoorkeur ontbreekt nog. Handmatige maaiveldinvoer
downloadt ontbrekende KickTheMap-jobdata nog niet automatisch.

Dit sluit ook de bredere [functiepariteitsvoorwaarden](PARITY.md) niet. De eerder
gepubliceerde previewrelease blijft bij zijn oorspronkelijke releasecommit.
