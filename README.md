# SleufBase Rust

Native Rust-versie van SleufBase voor Windows. De desktopapp heeft geen Python-runtime nodig.

De code staat nu in deze zelfstandige repository. **Dit is een migratiepreview:**
publicatie betekent niet dat alle oorspronkelijke functies en praktijksituaties
volledig zijn geaccepteerd. Zie [de overdracht en testresultaten](rust/HANDOFF_2026-10-26.md)
en [de volledige acceptatievoorwaarden](rust/PARITY.md).
De [nieuwste controles en browserverbeteringen](rust/VERIFICATION_2026-10-26_CONTROLS.md)
beschrijven de aanvullende tests en opgeloste fouten.

De export ondersteunt een projectnaam en vult de bladdatum automatisch in.
Cyclomedia-kaartachtergronden en het StreetSmart-venster zijn op verzoek verwijderd.

## Downloaden en starten

Open [Actions](https://github.com/LukaStuurman/SleufBase-RUST/actions), kies een
geslaagde **Rust Windows**-build en download **SleufBase-Rust-Windows-portable-preview**.
Pak de volledige ZIP uit en start `sleufbase-rust.exe`.
Dezelfde build levert een afzonderlijke Windows-previewinstaller.
Zie [de gebruiksinstructies](rust/PORTABLE.md).

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
Python is uitsluitend nodig voor de ontwikkelhulpmiddelen die pakketten controleren.

## Inhoud

- `rust/`: applicatie, tests, oorspronkelijke referentie-uitkomsten en installerbron.
- `assets/`: DXF-sjabloon, logo's en iconen.
- `tools/`: pakketcontrole en tests van de browsercapture.
- `SOURCE_MANIFEST.json`: herkomst en SHA-256 bij de eerste bronoverdracht;
  daaropvolgende verbeteringen zijn afzonderlijk in de Git-geschiedenis vastgelegd.

De oorspronkelijke Python-bron en onafhankelijke vergelijkingshulpmiddelen blijven
beschikbaar in [SleufBase](https://github.com/LukaStuurman/SleufBase).
Historische rapporten onder `rust/` verwijzen naar die bronrepository en hun eigen revisie.
