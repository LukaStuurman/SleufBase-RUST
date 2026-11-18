# SleufBase Rust

SleufBase Rust is een Windows-app voor proefsleuven, kabels en leidingen. Je brengt KickTheMap-jobs, GeoTIFF-beelden, KLIC-tekeningen, lokale LAS/LAZ-puntenwolken, marXact-metingen en zelf getekende proefsleuven samen op één kaart. Daarna maak je kaarten, DXF-sjabloontekeningen en discipline-overzichten. De desktopapp heeft geen Python-installatie nodig.

## Downloaden

- [Windows-installer](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest/download/SleufBase-Rust-Setup.exe): installeren en bijwerken via de installatiewizard.
- [Portable ZIP](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest/download/SleufBase-Rust-portable.zip): volledig uitpakken en `sleufbase-rust.exe` starten.
- [Laatste release](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest): wijzigingsnotities en SHA-256-controlebestanden.

Sluit SleufBase voordat je bijwerkt. Lees [installeren, bijwerken en portable gebruik](rust/PORTABLE.md) voor uitleg over de bestanden, WebView2 en het bewaren van instellingen.

## Handleidingen

De **[gebruikershandleiding](rust/HANDLEIDING.md)** beschrijft de bediening stap voor stap, inclusief de recentere kaart-, job-, invoer- en exportfuncties. In het programma open je de lokale uitleg met **Info** of **F1**.

| Ik wil… | Waar vind ik de uitleg? |
| --- | --- |
| Snel een project opbouwen | [Snel beginnen](rust/HANDLEIDING.md#1-snel-beginnen) |
| Navigeren, lagen kiezen of kabelinformatie bekijken | [Kaart en lagen](rust/HANDLEIDING.md#2-het-hoofdscherm-en-de-kaart) en [kabelinformatie](rust/HANDLEIDING.md#5-kabelinformatie-en-lijnselectie) |
| Jobs laden of op de jobskaart selecteren | [Jobs zoeken en laden](rust/HANDLEIDING.md#7-jobs-zoeken-selecteren-en-laden) |
| Kabelgegevens op de KickTheMap-webpagina invullen | [Apart KickTheMap-jobvenster](rust/HANDLEIDING.md#9-kabels-en-leidingen-in-de-kickthemap-browser) |
| marXact-namen koppelen of een virtuele sleuf bewerken | [marXact](rust/HANDLEIDING.md#11-marxact-metingen-importeren) en [virtuele proefsleuven](rust/HANDLEIDING.md#12-virtuele-proefsleuven-maken-en-bewerken) |
| Een DXF met kaarten en profielen maken | [DXF-sjabloonexport](rust/HANDLEIDING.md#15-dxf-sjabloonexport) |
| Een eigen sjabloon gebruiken of een back-up herstellen | [Eigen sjabloon](rust/HANDLEIDING.md#17-eigen-sjabloon-stempel-en-logos) en [bewaren en herstellen](rust/HANDLEIDING.md#19-instellingen-projecten-en-back-ups) |

## De dagelijkse werkwijze

1. **Gegevens laden.** Gebruik **KickTheMap → Jobs**, **Acties → Laad GeoTIFF(s)**, **Acties → Laad KLIC DXF** of **marXact → marXact import**.
2. **Op de kaart bekijken.** Kies de achtergrond boven de kaart. Dubbelklik op een proefsleuf of overlay om ernaartoe te zoomen. De vinkjes bepalen de zichtbaarheid; **Acties → Zoom naar extent** toont het overzicht van zichtbare gegevens.
3. **Gegevens aanvullen.** Open een job in het aparte KickTheMap-venster, bewerk een virtuele sleuf of voeg beginpunten, referenties, maaiveldinformatie en opmerkingen toe.
4. **Bewaren.** Gebruik **Acties → Project opslaan**. Het opslaan van een sleuf in het project en het opslaan van het projectbestand zijn afzonderlijke stappen.
5. **Exporteren.** Gebruik **Acties → Proefsleuven sjabloon DXF**. Controleer **Meenemen**, bronkoppelingen, volgorde, PS-nummer, vrije varianttekst en schaal. Bewaar bij het delen de DXF en de bijbehorende afbeeldingen samen.

Het KickTheMap-jobvenster heeft kabel- en leidingvelden **in de webpagina zelf**. Sla je wijzigingen op in KickTheMap en gebruik daarna **Objecten opnieuw laden** in Jobs om de lokale projectgegevens bij te werken. Dubbelklikken op een jobregel laadt de gegevens; dubbelklikken op een punt in de jobskaart opent het jobvenster. De handleiding legt deze bediening verder uit.

## Puntenwolken van andere leveranciers

Open **Puntenwolk → LAS/LAZ openen** en selecteer desgewenst meerdere bestanden tegelijk. Die worden achtereenvolgens in het catalogusoverzicht opgeslagen en kunnen later afzonderlijk worden bewerkt. Met complete begin- en eindpunten plaatst **Inladen** de sleuf meteen op de kaart. Voor bevestigde RD-scans maakt SleufBase na het plaatsen van begin- en eindpunt een gegeorefereerde **orthofoto in hoge resolutie** (tot 8.192 pixels / 36 megapixels), met de naam van de puntenwolk en zonder tekenannotaties. De DXF-sjabloonexport gebruikt die orthofoto in plaats van een kunstmatige virtuele proefsleufafbeelding.

Open de scan later met **Bewerken** in het puntenwolkenoverzicht om de eigen Potree-editor te gebruiken. SleufBase bereidt meerdere bestanden één voor één voor zonder die editor steeds automatisch te openen. Plaats **Beginpunt** en **Eindpunt** op het maaiveld om de proefsleufas vast te leggen. Teken daarna punten of polylijnen voor kabels en leidingen. De dropdowns **Type** en **Materiaal** gebruiken dezelfde instellingen als de normale sleufbewerker.

**Puntenwolken beheren** bewaart de scans ook na opnieuw starten of wisselen van project. De doorzoekbare tabel toont aantallen, de status van begin/eind en coördinaten, met **Bewerken** en **Inladen** per scan. De overzichtskaart toont bronnen met bevestigde RD-coördinaten. **Bewerken** wijzigt de bewaarde scan; alleen **Inladen** neemt nieuwe gegevens in het project over. Daarbij kies je de sleufnaam, breedte en objecten voor de doorsnede; begin/eind bepalen automatisch de as. De volledige tekening blijft daarnaast als bronlaag in het project beschikbaar. Een eerdere sleuf kun je bijwerken of als nieuwe kopie inladen. Meetpunten binnen de sleuf en werkelijke kruisingen van kabelpolylijnen komen in de gewone sleufbewerker en DXF-sjabloonexport terecht.

Kaart- en proefsleufintegratie vereisen bevestigde RD-coördinaten in meters. Bij andere stelsels blijven de originele XYZ behouden en kun je getekende objecten rechtstreeks naar DXF exporteren. Er wordt niet automatisch herprojecteerd.

De installer en portable ZIP bevatten de viewer en converter; import en tekenen vereisen geen online account. Sla ingeladen sleuven op in je Rust-project en bewaar de originele LAS/LAZ als je het werk naar een andere computer verplaatst. De lokale weergave kan daar opnieuw worden opgebouwd. Puntenwolken uit oudere projecten worden bij het openen aan het overzicht toegevoegd.

## Voor ontwikkeling

[Technische documentatie en historische migratienotities](rust/README.md) beschrijven de opbouw en eerdere vergelijkingen. De gedateerde rapporten leggen resultaten van hun eigen bronrevisie vast; ze vervangen de actuele gebruikershandleiding niet.

Zelf bouwen op Windows met Rust stable en de C++-buildtools:

```powershell
cargo build --locked --release --manifest-path rust/Cargo.toml --bin sleufbase-rust
.\rust\target\release\sleufbase-rust.exe
```

Voor lokaal gebruik tijdens ontwikkeling: voer `python tools/prepare_pointcloud_runtime.py --msvc-runtime-dir "<pad-naar-x64-Microsoft.VC143.CRT>"` uit. Gebruik de releasebibliotheken onder de Visual Studio-map `VC/Redist/MSVC/<versie>/x64`; de Windows-workflow zoekt die map automatisch op. Dit haalt de vastgelegde upstream-versies op, controleert hun SHA-256 en bundelt de vereiste converterbibliotheken. De applicatie downloadt tijdens gebruik geen viewer of converter.

Voor een compleet pakket zijn ook de meegeleverde logo's en licentiebestanden nodig. De Windows-workflow maakt de installer en de portable ZIP. Ontwikkelbuilds staan bij [Actions](https://github.com/LukaStuurman/SleufBase-RUST/actions).

## Repository

- `rust/`: applicatie, gebruikershandleiding, technische documentatie, tests en installerbron.
- `assets/`: DXF-sjabloon, logo's en iconen.
- `tools/`: bouw-, pakket- en uitvoerhulpmiddelen.
- `SOURCE_MANIFEST.json`: herkomst en controlesommen bij de oorspronkelijke bronoverdracht.

De oorspronkelijke Python-bron staat in [SleufBase](https://github.com/LukaStuurman/SleufBase). SleufBase Rust heeft eigen projectopslag en een eigen installatie. Ondersteuning van een oudere projectindeling betekent niet dat iedere functie van beide apps identiek is.
