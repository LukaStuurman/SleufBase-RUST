# SleufBase Rust installeren en bijwerken

SleufBase Rust is een native Windows-app voor 64-bits systemen. Je kunt de installer of de portable ZIP gebruiken. Voor het programma zelf hoef je geen Python te installeren.

Voor de bediening in het programma gebruik je **Info** of **F1**, of lees je de [gebruikershandleiding](HANDLEIDING.md).

## Installeren met de installer

1. Download **SleufBase-Rust-Setup.exe** van de [laatste release](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest).
2. Sluit een eventueel geopende SleufBase Rust.
3. Start de installer en volg de stappen. Je kunt een bureaubladsnelkoppeling laten maken.
4. Start SleufBase Rust via de snelkoppeling of het startmenu.

De standaardinstallatie staat onder `%LOCALAPPDATA%\Programs\Techbase\SleufBase-Rust`. De installer gebruikt een eigen installatie naast de oorspronkelijke Python-versie van SleufBase. Bij een volgende installatie kan dezelfde map worden gebruikt.

De installer bevat ook de installatievoorziening voor **Microsoft Edge WebView2 Runtime**. WebView2 is nodig voor de ingebouwde website-aanmelding en het aparte KickTheMap-jobvenster.

## Starten vanuit de portable ZIP

1. Download **SleufBase-Rust-portable.zip** van de releasepagina.
2. Pak de **hele ZIP** uit in een eigen map, bijvoorbeeld in Documenten. Start het programma niet rechtstreeks vanuit de ZIP-weergave.
3. Laat `assets/template_logos`, de licenties en de overige meegeleverde bestanden in de uitgepakte map staan.
4. Start `sleufbase-rust.exe`.

De portable ZIP bevat de WebView2 Runtime niet. Als de KickTheMap-websiteweergave een ontbrekende runtime meldt, gebruik dan de SleufBase-installer om de benodigde voorziening te laten installeren. Een internetverbinding blijft nodig voor online aanmelding, nieuwe downloads en online kaartgegevens.

De term *portable* betekent hier dat je de uitgepakte applicatie zonder een SleufBase-installatiewizard kunt starten. Instellingen, aanmeldingsgegevens, caches en automatische back-ups worden ook in de Windows-gebruikersmap opgeslagen. Het is dus geen volledige werkomgeving die uitsluitend naast de EXE staat.

## Bijwerken

1. Sla het geopende project op.
2. Sluit SleufBase Rust en het bijbehorende KickTheMap-venster.
3. Installeer de nieuwe installer, of pak de volledige nieuwe portable ZIP uit in een nieuwe programmamap.
4. Start de nieuwe versie en open het project via **Acties → Project laden**.

Je projectbestanden en oorspronkelijke meetbestanden staan los van de programmadownload. Bewaar de locaties waar je TIFF-, DXF- en andere bronbestanden hebt opgeslagen. Alleen een EXE vervangen is geen volledige update van een portable pakket.

Je ziet de versie van het gestarte programma onderaan **Info**. Dit helpt wanneer er meerdere uitgepakte versies of snelkoppelingen op je computer staan.

## Instellingen en bestanden bewaren

SleufBase bewaart zijn gebruikersgegevens standaard onder `%LOCALAPPDATA%\SleufBase\Rust`. Automatische back-ups staan daar in de map `Backups`. Gebruik **Instellingen → Back-ups** om autosave en herstel te beheren.

Een project kan naar externe bronbestanden verwijzen. Wil je op een andere computer verder, bewaar dan zowel het project als de gebruikte bestanden. De inloggegevens van KickTheMap horen bij het Windows-account en worden niet als onderdeel van het project gedeeld.

Bij de sjabloonexport horen naast de DXF ook afbeeldingsbestanden. Verplaats of deel de complete export met behoud van de mappenstructuur, zodat de verwijzingen vanuit de tekening blijven werken.

## KickTheMap gebruiken

Open **KickTheMap → Inloggen**. Met **Inloggegevens onthouden en automatisch aanmelden** bewaart SleufBase de aanmelding na succes versleuteld voor het huidige Windows-gebruikersaccount. Bij een volgende start probeert het programma de aanmelding automatisch te herstellen. **Uitloggen** verwijdert de bewaarde aanmelding en schakelt automatisch aanmelden uit.

Als KickTheMap een extra beveiligingscontrole vereist, rond je die af in de websiteweergave binnen SleufBase. De normale invoer van e-mailadres en wachtwoord gebeurt in het eigen inlogvenster.

Open vervolgens **KickTheMap → Jobs**. Kies één job en klik op **Open in KickTheMap-browser**; in een smal venster heet deze knop **Open browser**. De job opent in een apart venster. De kabel- en leidingkeuzes staan in de webpagina zelf. Sla daar eerst je wijzigingen op en gebruik daarna **Objecten opnieuw laden** in Jobs om de lokale gegevens bij te werken.

Lees de [stappen voor jobs en het browservenster](HANDLEIDING.md#7-jobs-zoeken-selecteren-en-laden) en de [uitleg over handmatige GeoTIFF-vastlegging](HANDLEIDING.md#10-handmatig-een-geotiff-vastleggen) voor de volledige werkwijze.

## Download controleren

Bij iedere release staan SHA-256-controlebestanden naast de installer en de ZIP. Je kunt bijvoorbeeld de ZIP-hash in PowerShell opvragen:

```powershell
(Get-FileHash .\SleufBase-Rust-portable.zip -Algorithm SHA256).Hash
```

Vergelijk de uitkomst met **SleufBase-Rust-portable.sha256**. Voor de installer gebruik je op dezelfde manier **SleufBase-Rust-Setup.exe** en **SleufBase-Rust-Setup.sha256**.

`build-info.json` in de portable map vermeldt de applicatieversie, bronrevisie en controlesommen van de pakketbestanden. De licenties zijn meegeleverd; ze zijn ook via `sleufbase-rust.exe --licenses` opvraagbaar.

## Als starten of bijwerken niet lukt

- **Een oud versienummer in Info:** controleer welke snelkoppeling of uitgepakte EXE je gestart hebt.
- **Bestanden ontbreken:** pak de volledige ZIP opnieuw uit; behoud de map `assets` en de meegeleverde bestanden.
- **KickTheMap-jobvenster kan niet worden gemaakt:** lees de statusmelding. Gebruik bij een ontbrekende WebView2 Runtime de installer.
- **Een TIFF is na het openen van een project niet beschikbaar:** controleer of het oorspronkelijke bestand nog op de opgeslagen locatie staat.
- **Een export mist afbeeldingen in CAD:** zet de bijbehorende afbeeldingsmap terug naast de export met de oorspronkelijke mappenstructuur.

Meer uitleg staat onder [Problemen oplossen](HANDLEIDING.md#21-problemen-oplossen).
