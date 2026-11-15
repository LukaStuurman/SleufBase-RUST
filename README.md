# SleufBase Rust

Native Rust-versie van SleufBase voor Windows. De desktopapp heeft geen Python-runtime nodig.

De code staat nu in deze zelfstandige repository. **Dit is een migratiepreview:**
publicatie betekent niet dat alle oorspronkelijke functies en praktijksituaties
volledig zijn geaccepteerd. Zie [de overdracht en testresultaten](rust/HANDOFF_2026-10-26.md)
en [de volledige acceptatievoorwaarden](rust/PARITY.md).
De [browsercontroles](rust/VERIFICATION_2026-10-26_CONTROLS.md) en
[parallelle exportvoorbereiding met PNG-herstel](rust/VERIFICATION_2026-10-26_ASSETS.md)
beschrijven de aanvullende tests en opgeloste fouten.

De export ondersteunt een projectnaam en vult de bladdatum automatisch in.
Cyclomedia-kaartachtergronden en het StreetSmart-venster zijn op verzoek verwijderd.
Ook de Gemma-AI-functie is verwijderd. Handmatig maaiveldteksten en lijnkleuren
invullen en AHN-hoogtes aanvullen blijven beschikbaar.

## Downloaden en starten

Download de [Windows-installer](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest/download/SleufBase-Rust-Setup.exe)
of de [portable ZIP](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest/download/SleufBase-Rust-portable.zip)
van de [laatste release](https://github.com/LukaStuurman/SleufBase-RUST/releases/latest).
Sluit SleufBase voor het bijwerken. Pak bij de portable versie de volledige ZIP uit
en start `sleufbase-rust.exe`.
Ontwikkelbuilds staan bij [Actions](https://github.com/LukaStuurman/SleufBase-RUST/actions)
in de workflow **Rust Windows**.
Zie [de gebruiksinstructies](rust/PORTABLE.md).

## Dagelijks gebruik

- **KickTheMap:** open **KickTheMap → Inloggen**, meld je aan en laat **Inloggegevens onthouden en automatisch aanmelden** aangevinkt als je de aanmelding op dit apparaat wilt bewaren. Een bewaarde aanmelding wordt bij het starten op de achtergrond hersteld. Bij een verplichte beveiligingscontrole gebruik je de ingebouwde websiteweergave. **Uitloggen** schakelt automatisch aanmelden uit.
- **Virtuele proefsleuf:** teken de sleuf op de kaart en open de editor. Klik in de doorsnede om een positie te kiezen, vul type en afmetingen in en kies **Kabel/leiding toevoegen**. Met **Zelfde type toevoegen** zet je de eigenschappen klaar voor de volgende kabel; kies daarna de nieuwe positie. **Opslaan in project** bewaart de volledige sleuf, inclusief actieve invoer. **Sluiten zonder opslaan** verwerpt het concept.
- **DXF-sjabloon:** kies projectgegevens en een nieuwe uitvoer-DXF, vink de gewenste sleuven aan en controleer de samenvatting. De standaardkaart gebruikt een luchtfoto; offline wordt een witte achtergrond met al geladen gegevens gebruikt. Exacte schaal, resolutie, eigen sjabloon, logo's en lettertypen staan onder **Meer exportinstellingen**. Bronkeuze, profielschaal, foto's en opmerkingen zijn per sleuf beschikbaar.

## Zelf bouwen

Installeer Rust stable op Windows met de benodigde C++-buildtools. Voer vanuit
deze repository uit:

```powershell
cargo test --locked --manifest-path rust/Cargo.toml --all-targets --jobs 2
cargo build --locked --release --manifest-path rust/Cargo.toml --bin sleufbase-rust
.\rust\target\release\sleufbase-rust.exe
```

De map `assets/template_logos` moet naast de executable staan voor de standaardlogo's.
De CI maakt hiervoor een volledig pakket met licenties en SHA-256-controlebestand.
Python is alleen nodig voor ontwikkelhulpmiddelen, zoals pakket- en uitvoercontroles.

## Inhoud

- `rust/`: applicatie, tests, oorspronkelijke referentie-uitkomsten en installerbron.
- `assets/`: DXF-sjabloon, logo's en iconen.
- `tools/`: pakketcontrole, browserregressies en vergelijking van echte exportuitvoer.
- `SOURCE_MANIFEST.json`: herkomst en SHA-256 bij de eerste bronoverdracht;
  daaropvolgende verbeteringen zijn afzonderlijk in de Git-geschiedenis vastgelegd.

De oorspronkelijke Python-bron en onafhankelijke vergelijkingshulpmiddelen blijven
beschikbaar in [SleufBase](https://github.com/LukaStuurman/SleufBase).
Historische rapporten onder `rust/` verwijzen naar die bronrepository en hun eigen revisie.
