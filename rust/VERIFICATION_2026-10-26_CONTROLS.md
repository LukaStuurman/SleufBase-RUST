# Browsercontroles en herstel, 26 oktober 2026

De controles vonden een concrete fout in de gewone browserfallback: de oude
wachtlus had alleen een timeout per socketbewerking. Doorlopend binnenkomende
browsermeldingen konden de totale wachttijd blijven verlengen. De annuleervlag
van de download werd bovendien niet aan deze fallback doorgegeven.

## Hersteld

- Browseraanmelding, gewone downloadfallback en handmatige capture gebruiken
  één begrensde CDP-verbinding. De absolute deadline geldt ook tijdens verbinden,
  handshake, gedeeltelijke berichten en schrijven. Gewone opdrachten hebben
  maximaal 15 seconden; capture behoudt 4 seconden en korte cleanupbudgetten.
- Annuleren wordt doorgegeven vanuit de downloadworker, ook aan de tabselectie.
  Nonblocking sockets controleren tijdens wachten regelmatig de annuleervlag.
  Bij schrijfdruk wordt hetzelfde bericht verder verstuurd; een opdracht wordt
  niet opnieuw ingediend.
- Verbindingsdoel en poort blijven beperkt tot de eigen lokale Edge-instance.
  Tabselectie vereist de exacte KickTheMap-origin en een geldige page-ID.
  De JavaScript-helper controleert de origin opnieuw vóór de aanvraag, zodat
  navigatie tussen tabselectie en uitvoeren geen aanvraag op een vreemde site maakt.
- CDP controleert bericht- en framelimieten vóór volledige payloadontvangst.
  Opdrachten zijn begrensd op 1 MiB; URL-/capture-antwoorden op 512 KiB.
  De bestaande grotere joblijstlimiet van 24 MiB blijft behouden.
- Het aanvragen van een downloadlink in de pagina zelf heeft een aborttimer
  van 14 seconden, een antwoordbudget van 256 KiB en maximaal 2048 chunks.
  Streams en timers worden opgeruimd. De oorspronkelijke multipart-, form- en
  JSON-fallbacks en ondersteunde antwoordvormen blijven beschikbaar.

## Testbewijs

De nieuwe Rust-regressies gebruiken echte lokale TCP-/WebSocketverbindingen:
gebeurtenisflood, afgebroken handshake, annuleren tijdens handshake/antwoord,
aangekondigde en gefragmenteerde oversize frames, te grote opdrachten,
schrijfdruk en de werkelijke publieke downloadlinkfallback. Die laatste
controleert tabselectie, verkeerde poorten, onveilige downloaddomeinen en annuleren.
De bestaande capture-/cleanupregressies blijven van toepassing.

De echte JavaScript-helper wordt uitgevoerd in een geïsoleerde Node-context.
Twaalf scenario's controleren de drie aanvraagvormen, antwoordvarianten,
byte-/chunkbudgetten, ontbrekende/corrupte inhoud, timeouts en cleanup.
Vreemde websites en afwijkende HTTPS-poorten leveren nul fetchaanroepen op.
De negen bestaande capture-scenario's slagen eveneens.

De afzonderlijke medium-review van de instellingenopslag vond geen nieuwe fout.
Een tweede medium-review van het browserpad vond de navigatierace; deze is
hersteld en met een regressie afgedekt. De hoofdreview heeft de integratie
gecontroleerd en de volledige native testset en Clippy uitgevoerd.

| Lokale controle | Resultaat |
| --- | --- |
| Volledige Rust-testset, alle doelen, vergrendeld/offline | 790 geslaagd over 151 doelen; 0 mislukt |
| Bibliotheek / desktop binnen die set | 283 / 130 geslaagd |
| Clippy, alle doelen | Geslaagd; bestaande waarschuwingen blijven zichtbaar |
| Echte JavaScript-helpers | 21 scenario's geslaagd: 9 capture, 12 downloadlink |
| Portable pakketregressies | 6 geslaagd |
| Browserregressies na de laatste origincontrole | 15 geslaagd |
| Actuele executable opnieuw bouwen | Geslaagd; ingebouwde helper bijgewerkt |

De aanvullende origincontrole in de laatste JavaScript-review is afzonderlijk
op de actuele helper getest; de browserregressies en executable zijn daarna
opnieuw opgebouwd met diezelfde ingesloten helper.

De eerste zelfstandige Windows-build bij revisie `4d33eeb` is volledig groen:
[Windows-run 40269300409](https://github.com/LukaStuurman/SleufBase-RUST/actions/runs/40269300409).
De ZIP en installer zijn opnieuw gedownload; beide GitHub-archiefdigests en de
interne controlehashes kloppen. Alle dertien portable bestanden zijn gecontroleerd.
De werkelijk gedownloade MSVC-executable start met `--help` en schrijft een
onafhankelijk gecontroleerde DXF met vijf entities en Excel met drie rijen.
De installer is gecompileerd en op hash/PE gecontroleerd; hij is lokaal niet
geïnstalleerd. Zie [het artefactbewijs](verification/windows-artifact-4d33.json).
Deze oudere groene build wordt niet aan de nieuwere browserwijzigingen toegeschreven.

## Praktische grens

Deze tests starten geen echte Edge-aanmelding en gebruiken geen accounts of
persoonlijke instellingen. Ze bewijzen de lokale protocol-, budget- en
annuleercontracten. Echte dienst-/accountacceptatie en de overige voorwaarden
in [PARITY.md](PARITY.md) blijven open. Parallelle exportassetpools en bilineaire
rasterverkleining vallen buiten deze herstelronde.
