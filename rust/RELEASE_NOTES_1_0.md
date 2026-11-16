## SleufBase Rust 1.0.11 voor Windows

### Grote KLIC-DXF-bestanden laden en bewaren

- De DXF-import berekent geen lettercontouren meer voor intrinsiek onzichtbare tekst en blokattributen. Grote KLIC-tekeningen met veel verborgen GIS-attributen kunnen daardoor laden zonder het tekstbudget onnodig te overschrijden.
- Attribuutnamen en -waarden blijven beschikbaar als bron- en inspectiegegevens. De bestaande grenzen voor geometrie en zichtbare tekst blijven actief.
- Native projecten worden compacter opgeslagen en kunnen tot 512 MiB groot zijn. Lezen, herstel en metadata-inspectie verwerken de invoer gebufferd, zodat geen extra volledige kopie van het JSON-bestand in het geheugen nodig is.
- Opslaan blijft atomisch en begrensd. De oorspronkelijke limiet voor Python-projecten blijft 128 MiB.

### Meer invulvelden in DXF-sjabloonexport

In het exportvenster staat een sectie **Tekeningstempel** met:

- **Gezien**
- **Status**
- **Versie** — standaard **1.0**
- **Referentie Tekeningen** — standaard **Proefsleuven**
- **ProjectNr.BGM**
- **TEK.NR.**

De ingevulde waarden worden doorgegeven aan de overeenkomstige stempelvakken op de geëxporteerde bladen en onthouden in het native project. De bestaande projectnaam, tekenaar en latere revisieregels blijven behouden. De technische AutoCAD-keuze voor een normale of omgekeerde weergave staat los van het invulveld Versie.

### Instellingen consequent toepassen

- Het kadastrale overzicht in de DXF-sjabloonexport volgt de actuele keuze **Geen / Hartlijn / Vlak** en **GeoTIFF-afbeeldingen meenemen**. Een eerder opgeslagen export overschrijft deze instellingen niet meer. De vaste detail-, kaart- en profielvakken blijven afzonderlijk instelbaar.
- Ook de labelafstand en -kleuren, achtergrond, KLIC-lagen en profielopmaak worden correct doorgegeven. **OpenStreetMap** en **Kadaster** zijn nu echte achtergrondkeuzes voor sjabloonkaarten.
- **Ook een omgekeerde versie exporteren** bepaalt daadwerkelijk of de tweede versie wordt gemaakt. Extra kabel-/leidingkeuzes verschijnen in de Type-keuzelijst.
- MarXact-naamkoppelingen, automatisch laden van TIFF-beginpunten en het maken van een virtuele kopie uit KickTheMap gebruiken de effectieve projectinstellingen.
- De instellingenkoppelingen zijn in de broncode nagelopen; er zijn hiervoor geen runtime- of geautomatiseerde tests uitgevoerd.

### Actuele Techbase-contactgegevens op de bladen

De vaste contactregels in alle gebruikte stempelvarianten worden bijgewerkt naar de [officiële contactpagina](https://techbasebv.nl/contact): **De Bloemendaal 10**, **5221 EC 's-Hertogenbosch**, **073 220 0341** en **www.techbasebv.nl**. Het algemene e-mailadres op die pagina is **contact@hdbtechbase.nl**; dat blijft in de stempel staan. De correctie geldt ook voor gedeelde en dynamische onderhoekblokken.

Onder **Instellingen > DXF-export > Contactgegevens op DXF-bladen** zijn adres, postcode, plaats, telefoon, e-mail en website aanpasbaar. Lege velden laten de betreffende contactregel weg. Met **Standaard Techbase-gegevens herstellen** worden de meegeleverde waarden teruggezet. De opgeslagen instellingen worden doorgegeven aan alle gebruikte sjabloonbladen en in het native project bewaard.

### Snellere en scherpere OpenStreetMap-werkkaart

- De werkkaart gebruikt een afzonderlijke tegelroute op basis van de fysieke schermresolutie, met een scherper gekozen zoomniveau tot het beschikbare OSM-detail.
- Tegels uit de lokale cache worden eerst getoond. Ontbrekende tegels worden met maximaal twee gelijktijdige downloads opgehaald; de extra wachttijd van 300 ms per tegel is voor de werkkaart verwijderd.
- De achtergrond wordt tijdens het laden geleidelijk aangevuld. Bij verschuiven en zoomen blijft de vorige bruikbare kaart zichtbaar waar die overlapt met het nieuwe kaartgebied.
- Alleen het zichtbare kaartgebied wordt opgehaald. Geheugen, tegelverzoeken en tussentijdse beelden blijven begrensd; verlopen cachetegels worden waar mogelijk met HTTP-herbevestiging hergebruikt.

### Volgorde aanpassen zonder terug te scrollen

**Omhoog** en **Omlaag** staan in een vaste bedieningskolom rechts, buiten het scrollende exportformulier. Na verplaatsen blijft de geselecteerde proefsleuf in beeld. De export- en annuleerknoppen blijven onderaan bereikbaar.

### Knoppen bij Proefsleuven

**Virtuele proefsleuf** staat naast **Verwijder selectie**. Het aantal geselecteerde items staat erboven; de selectie met Ctrl en Shift blijft beschikbaar.

### Build en release

De geautomatiseerde tests, Clippy en installatieproeven worden op verzoek overgeslagen. De Windows-app, portable ZIP en installer worden gebouwd; de releasebestanden worden gecontroleerd op versie, inhoud en SHA-256-controlesom voor publicatie.

### Downloaden en bijwerken

Gebruik **SleufBase-Rust-Setup.exe** om de Windows-app bij te werken, of download **SleufBase-Rust-portable.zip** voor gebruik zonder installatie. Sluit SleufBase voor het bijwerken en pak de portable ZIP volledig uit voordat je het programma start.
