//! Local, topic-based help for the working map and the existing desktop flows.
use super::{egui, theme, Color32, INK, ORANGE};

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    query: String,
    selected: usize,
    focus_search: bool,
}

impl State {
    pub fn open(&mut self) {
        self.open = true;
        self.focus_search = true;
    }
}

struct Topic {
    title: &'static str,
    summary: &'static str,
    keywords: &'static str,
    sections: &'static [(&'static str, &'static str)],
}

const TOPICS: &[Topic] = &[
    Topic {
        title: "Snel beginnen",
        summary: "Van bronbestanden naar een overzichtelijk project en een export.",
        keywords: "start eerste stappen workflow handleiding",
        sections: &[
            ("1. Kies je gegevens", "Open KickTheMap > Jobs om gemeten proefsleuven te laden. Lokale beelden laad je via Acties > Laad GeoTIFF(s), een KLIC via Acties > Laad KLIC DXF. Gebruik marXact > marXact import voor een MarXact-meting."),
            ("2. Bekijk de kaart", "In Proefsleuven links kies je een sleuf. Dubbelklik op een regel om ernaartoe te zoomen. Met de vinkjes in Proefsleuven en Overlays toon of verberg je de betreffende laag. Met Acties > Zoom naar alles zoom je naar de zichtbare gegevens."),
            ("3. Vul de sleuf aan", "Rechtsklik op een sleuf in de lijst of op de kaart voor de beschikbare bewerkingen. Met Virtuele proefsleuf onder de lijst teken je een nieuwe sleuf en voeg je zelf kabels en leidingen toe."),
            ("4. Bewaar en exporteer", "Bewaar je werk via Acties > Project opslaan. Gebruik Acties > Proefsleuven sjabloon DXF voor de tekening met kaarten en profielen. Kies hieronder het onderwerp waar je meer over wilt weten."),
        ],
    },
    Topic {
        title: "Kaart en naambordjes",
        summary: "Navigeren, lagen bekijken en de namen van je proefsleuven herkennen.",
        keywords: "kaart labels namen naam naamlabels naambordjes zoom muis scroll lagen achtergrond tiff transparantie coordinaten",
        sections: &[
            ("Zoomen en verschuiven", "Scroll met de muis boven de kaart om in of uit te zoomen. Sleep met de middelste muisknop om te verschuiven. Met de linkermuisknop kan dat ook zolang er geen teken- of plaatsgereedschap actief is. Locatie zoekt een adres of RD-/GPS-coördinaat."),
            ("Achtergrond en lagen", "Onder Weergave kies je de achtergrondkaart. De TIFF-schuif maakt de beelden meer of minder doorzichtig. De vinkjes in Proefsleuven tonen of verbergen TIFF-, virtuele en MarXact-sleuven. De vinkjes in Overlays doen dit voor objectlagen en tekeningen. Met Jobpunten naast Verwijder selectie toon of verberg je alle jobpunten tegelijk. DXF-tekeningen behouden hun eigen zichtbaarheid. Klik op een zichtbare kabel om die met een gloed te markeren en de gegevens in Kabelinformatie te lezen."),
            ("Zichtbaarheid bewaren", "Een verborgen proefsleuf blijft in het project. Het vinkje verandert alleen de werkkaart, inclusief het naambordje; kies in de export apart welke proefsleuven meegaan. Sla het project op om de zichtbaarheid van de lagen bij een volgende keer te behouden."),
            ("Meerdere regels selecteren", "In Proefsleuven en Overlays voegt Ctrl+klik een losse regel toe of haalt deze uit de selectie. Shift+klik kiest een aaneengesloten reeks; Ctrl+Shift+klik voegt een reeks toe aan je bestaande selectie. Een gewone klik kiest weer één regel."),
            ("Selectie verwijderen", "Verwijder de geselecteerde regels samen met de verwijderknop onder de lijst, via rechtsklik > Verwijder selectie, of met Delete terwijl de lijst actief is. Rechtsklikken op een al geselecteerde regel behoudt de groep. Bronbestanden op schijf blijven behouden."),
            ("Naambordjes naast de sleuf", "KickTheMap/GeoTIFF-, MarXact- en virtuele proefsleuven krijgen naambordjes in de werkkaart. Ze houden een leesbare grootte tijdens zoomen en zoeken vrije ruimte naast de beelden en sleuven. Is er geen ruimte zonder overlap, dan blijft dat bordje tijdelijk verborgen. Zoom of verschuif de kaart om meer ruimte te maken."),
            ("Naambordjes aan of uit", "Ga naar Instellingen > Kaarten > Kaartscherm en wijzig Naambordjes van proefsleuven tonen. Klik op Opslaan om je keuze toe te passen en te onthouden. Deze instelling geldt voor de werkkaart; de opmaak van exportkaarten heeft eigen instellingen."),
        ],
    },
    Topic {
        title: "KickTheMap en jobs",
        summary: "Aanmelden, jobs vinden en de bijbehorende bestanden laden.",
        keywords: "login inloggen account wachtwoord automatisch onthouden aanmelden objectpunten downloaden filter joblijst internet cache",
        sections: &[
            ("Aanmelden en onthouden", "Open KickTheMap > Inloggen en vul je accountgegevens in. Met Inloggegevens onthouden en automatisch aanmelden bewaart SleufBase na een geslaagde aanmelding de gegevens versleuteld en probeert het bij de volgende start opnieuw aan te melden."),
            ("Extra controle bij aanmelden", "Vraagt KickTheMap om een extra controle, rond die dan af in het ingebouwde venster. Onder Andere manier van aanmelden kun je Aanmelden via KickTheMap-pagina kiezen. Uitloggen verwijdert de bewaarde aanmelding en zet automatisch aanmelden uit."),
            ("Jobs zoeken en kiezen", "Open KickTheMap > Jobs. Filter met de velden bij de kolommen of via Filter / selectie. Klik op een kolomkop om te sorteren. Lijst en Kaart bieden twee weergaven van de jobs; Vernieuwen haalt de lijst opnieuw op."),
            ("Bestanden in het project laden", "Selecteer de gewenste jobs en klik op Laad geselecteerde jobs voor objectpunten en GeoTIFF-beelden. Je kunt ook alleen Objectpunten downloaden + laden of GeoTIFF downloaden + laden kiezen. Dubbelklikken op een job laadt beide. Objectpunten verschijnen onder Overlays."),
            ("Als een download niet lukt", "Lees de statusmelding of open Downloadfouten. Meer opties en probleemoplossing bevat aanvullende herstelmogelijkheden. Offline cache toont eerder bewaarde jobinformatie; voor nieuwe downloads heb je een werkende aanmelding en internet nodig."),
        ],
    },
    Topic {
        title: "GeoTIFF en KLIC",
        summary: "Eigen beelden en kabeltekeningen op dezelfde kaart gebruiken.",
        keywords: "tif tiff raster import bestand dxf klic overlay orthofoto adres rd kabelinformatie xdata infracad laag selectie gloed uitzoomen snelheid details",
        sections: &[
            ("GeoTIFF-beelden laden", "Kies Acties > Laad GeoTIFF(s) en selecteer één of meer .tif- of .tiff-bestanden. SleufBase gebruikt hun geografische ligging. Bewaar eventuele bijbehorende .job.json-bestanden naast de TIFF, zodat de KickTheMap-jobinformatie beschikbaar blijft."),
            ("KLIC-tekeningen laden", "Kies Acties > Laad KLIC DXF. De tekening komt onder Overlays te staan. Met de vinkjes toon of verberg je tekeningen en lagen. Door in te zoomen kun je dicht bij elkaar liggende kabels preciezer selecteren."),
            ("Grote KLIC-overzichten", "Bij uitzoomen gebruikt de werkkaart een lichtere overzichtsweergave. Details die kleiner zijn dan een beeldpunt worden vereenvoudigd en onleesbaar kleine teksten verschijnen weer zodra je verder inzoomt. Tijdens zoomen en verschuiven beweegt het bestaande overzicht direct mee; het wordt kort na de beweging opnieuw scherp opgebouwd. De oorspronkelijke kabelgegevens blijven beschikbaar voor selectie, Kabelinformatie en export."),
            ("Kabelinformatie en selectie", "Klik op een kabel in de kaart. De geselecteerde lijn krijgt een gloed; de oorspronkelijke kabelkleur blijft in het midden zichtbaar. Kabelinformatie toont XDATA InfraCAD en Laag bovenaan, gevolgd door de overige objectgegevens. Bij overlappende kabels kies je met de wisselknop naast Kabelinformatie de volgende kabel. Via Lijst kies je direct een kabel aan de hand van de laagnaam en XDATA InfraCAD. De knoppen blijven boven het scrollende informatievak staan; na wisselen begint de informatie weer bovenaan. Klik naast de kabels om de kabelselectie op te heffen."),
            ("Een beeld terugvinden", "Zet het vinkje van de sleuf in Proefsleuven aan en dubbelklik op de regel om erop in te zoomen. Acties > Toon TIFF-adressen helpt bij het overzicht van de locaties. Met Acties > Zoom naar alles ga je terug naar een overzicht van de zichtbare gegevens."),
            ("Bewerken vanuit de lijst", "Rechtsklik op een TIFF voor onder meer draaien, opmerkingen, een virtuele kopie en het kopiëren van het midden. Een virtuele kopie kun je daarna aanvullen met eigen meetpunten en kabels of leidingen."),
        ],
    },
    Topic {
        title: "MarXact-metingen",
        summary: "Gemeten sleuven importeren en de kabel- of leidingoriëntatie afstemmen.",
        keywords: "marxact marXact import meting 3d polyline richting rotatie namen koppeling",
        sections: &[
            ("Een meting importeren", "Ga naar marXact > marXact import en kies de DXF-bestanden. Gevonden proefsleuven komen in het project en in Proefsleuven terecht. Ze hebben ook een eigen naambordje op de werkkaart."),
            ("Namen koppelen", "Onder Instellingen > KickTheMap kun je MarXact-namen aan kabel- of leidingnamen koppelen. De woordenlijst bepaalt daarnaast hoe herkende objecten worden benoemd en op DXF-lagen terechtkomen."),
            ("Oriëntatie controleren", "Open marXact > Oriëntatie aanpassen. Kies de sleuf en pas zo nodig de Rotatiecorrectie aan met de schuif, het getal of de kleine stappen. Correctie 0° verwijdert de extra correctie; Automatisch opnieuw bepalen berekent de uitlijning opnieuw vanuit de meting."),
            ("Gemeten gegevens bewerken", "Via het contextmenu kun je het bewerkscherm openen. De oorspronkelijke 3D-meetgrenzen en uitlijning worden beschermd. Kies Handmatige kopie als je een afzonderlijke bewerkbare versie met een gewijzigde as nodig hebt. Sla de gewenste wijzigingen daarna in het project op."),
        ],
    },
    Topic {
        title: "Virtuele proefsleuven",
        summary: "Een nieuwe sleuf tekenen en de vorm en gegevens instellen.",
        keywords: "virtueel maken tekenen begin eind breedte naam verplaatsen draaien kopie",
        sections: &[
            ("Een sleuf tekenen", "Klik onder Proefsleuven op Virtuele proefsleuf. Klik op de kaart eerst het beginpunt en daarna het eindpunt. Houd minimaal 0,5 meter tussen de punten. Escape of een rechtsklik annuleert het tekenen."),
            ("Naam, ligging en breedte", "Open via het contextmenu Virtuele proefsleuf bewerken. Op Sleufgegevens stel je de naam, breedte en begin- en eindcoördinaten in. Je kunt de begin- en eindrichting omkeren en de sleuf verschuiven. Op Doorsnede plaats en bewerk je kabels en leidingen."),
            ("Bewerkingen op de kaart", "Het contextmenu bevat verplaatsen, interactief draaien, een hoek invullen en een virtuele kopie plaatsen. Ook kun je een punt op de kaart of op een opgegeven kettingmaat toevoegen. Kettingmaat is de afstand langs de sleuf vanaf het beginpunt."),
            ("Wijzigingen bewaren", "Het bewerkscherm werkt met een concept. Opslaan in project neemt de wijzigingen over; Sluiten zonder opslaan bewaart ze niet. Bewaar vervolgens het projectbestand via Acties > Project opslaan."),
        ],
    },
    Topic {
        title: "Kabels en leidingen",
        summary: "Positie, diameter, diepte en kruishoek invullen in het bewerkscherm.",
        keywords: "doorsnede dwarsprofiel kabel leiding diameter materiaal diepte kruishoek aantal toevoegen herhalen z hoogte dekband",
        sections: &[
            ("Plaats in de doorsnede", "Open Proefsleuf bewerken > Doorsnede. Klik een plek in de visuele doorsnede en kies de eigenschappen. Met Kies punt op de kaart kun je de plaats ook in de werkkaart aangeven. De afstand vanaf het begin- of eindpunt helpt om nauwkeurig te plaatsen."),
            ("Type en afmetingen", "Kies het kabel- of leidingtype en vul waar bekend materiaal, diameter en aantal in. De weergegeven eenheden staan bij de invoervelden. Stel de kruishoek in met het getal of door de lijn in het kleine bovenaanzicht te slepen."),
            ("Diepte en hoogte", "Gebruik Diepte invullen voor een diepte onder het beginmaaiveld. Een Z-hoogte is een absolute hoogte; dit is een ander gegeven dan de diepte. Aanvullende velden bieden onder meer naam, bron/typecode, dwarsafstand en attributen."),
            ("Toevoegen, herhalen en wijzigen", "Klik op Kabel/leiding toevoegen om de invoer aan het concept toe te voegen. Toevoegen en herhalen en Zelfde type toevoegen nemen eigenschappen mee naar het volgende object; kies daarbij een nieuwe positie. Selecteer een bestaand object om het te wijzigen. Opslaan in project bewaart de hele sleuf."),
            ("Dekbanden en Excel", "Via Dekbanden beheren of Sleufgegevens kun je segmenten met begin- en eindafstand, omschrijving en diepte toevoegen. Het bewerkscherm biedt ook het importeren van geplakte Excel-regels en het kopiëren van meetobjecten naar Excel; volg de aangegeven kolomindeling."),
        ],
    },
    Topic {
        title: "Punten en opmerkingen",
        summary: "Referenties, beginpunten, maaiveld en coördinaten bij een sleuf vastleggen.",
        keywords: "referentiepunt beginpunt maaiveld ahn hoogte bgt opmerking coordinaten coördinaten kopiëren midden clipboard",
        sections: &[
            ("Begin- en referentiepunt", "Selecteer eerst de juiste sleuf. Met Beginpunt en Referentiepunt boven de kaart start je de bijbehorende plaatsing. Volg de melding onder in het scherm. Acties > Laad alle beginpunten haalt beschikbare beginpunten op; Wis alle referentiepunten verwijdert de opgeslagen referenties."),
            ("Maaiveld invullen", "Maaiveld invullen boven de kaart helpt om de maaiveldbeschrijving voor het profiel op de juiste plaats te kiezen. In Instellingen > Proefsleuven stel je de bijbehorende opties en BGT-aanvulling in. Het sleufbewerkvenster biedt ook hoogtegegevens; controleer of je een maaiveldbeschrijving, diepte of Z-hoogte invoert."),
            ("Een opmerking toevoegen", "Rechtsklik op de sleuf en kies Opmerking bewerken. Voor TIFF-opmerkingen wordt elke regel apart in de export opgenomen. Met Opmerking wissen verwijder je de tekst van die sleuf; Acties > Wis opmerkingen werkt voor het project."),
            ("Het midden kopiëren", "Rechtsklik op een sleuf in de lijst of in de kaart en kies Coördinaten midden kopiëren. Het klembord bevat de twee RD-getallen met een komma ertussen en zonder X- of Y-tekst, bijvoorbeeld 123456.000, 456789.000."),
        ],
    },
    Topic {
        title: "Kaarten en DXF exporteren",
        summary: "Kaartbeelden, sjabloontekeningen, dwarsprofielen en Excel-uitvoer maken.",
        keywords: "export kaarten dxf sjabloon profiel dwarsprofielen excel dpi resolutie legenda logo kadastraal gezien status versie referentie tekeningen projectnr bgm tek.nr tekeningstempel techbase contact adres postcode telefoon email e-mail website",
        sections: &[
            ("Losse kaarten", "Kies Acties > Export kaarten en selecteer een uitvoermap. De kaartuitvoer gebruikt de exportinstellingen onder Instellingen > Kaarten. Kies de gewenste achtergrond en kwaliteit; online achtergronden moeten tijdens het maken kunnen worden opgehaald."),
            ("Proefsleuven sjabloon DXF", "Open Acties > Proefsleuven sjabloon DXF. Vul projectnaam en tekenaar in en gebruik Uitvoerbestand kiezen… voor een nieuw uitvoerbestand. Stel onder Kaarten en profielen de achtergrond en kaartkwaliteit in. Kies bij Proefsleuven kiezen welke sleuven en profielen worden meegenomen en klik op Export starten."),
            ("Tekeningstempel invullen", "Onder Tekeningstempel vul je Gezien, Status, Versie, Referentie Tekeningen, ProjectNr.BGM en TEK.NR. in. Versie begint op 1.0 en Referentie Tekeningen op Proefsleuven. De gegevens komen in de overeenkomstige vakken van ieder gebruikt exportblad. Sla het Rust-project op om deze invoer te bewaren."),
            ("Volgorde en inhoud controleren", "KickTheMap-, MarXact- en virtuele sleuven kunnen in dezelfde sjabloonexport voorkomen. Controleer de volgorde, het PS-nummer en de vinkjes Meenemen. Selecteer een sleuf en gebruik Omhoog of Omlaag bovenaan de vaste kolom rechts. Daaronder staan ook sorteren, selectie, nummering en schaal, lege en extra vakken en acties voor alle sleuven. Dit paneel scrolt los van de sleufkaarten; de verplaatsknoppen blijven bereikbaar. Bij Kaarten en profielen kies je onder meer de kijkrichtingen, kaartlagen en een eigen DXF-sjabloon."),
            ("Weergave van het DXF-overzicht", "Instellingen > DXF-export bepaalt ook het kadastrale overzicht in de sjabloonexport. Kies Geen om de sleufvlakken of hartlijnen en hun namen weg te laten. GeoTIFF-afbeeldingen meenemen bepaalt apart of de beelden in dat overzicht komen: ook bij Geen kun je de TIFFs dus zichtbaar houden zonder proefsleufnamen eromheen. De vaste detail-, kaart- en profielvakken per sleuf stel je afzonderlijk in het exportvenster in."),
            ("Contactgegevens op de bladen", "Ga naar Instellingen > DXF-export > Contactgegevens op DXF-bladen om adres, postcode, plaats, telefoon, e-mail en website in de Techbase-onderhoek aan te passen. Laat een veld leeg om die regel weg te laten. Met Standaard Techbase-gegevens herstellen zet je de meegeleverde gegevens terug. Klik op Opslaan en sla ook het project op om projectgebonden wijzigingen te bewaren."),
            ("Objecten, percelen en Excel", "KickTheMap > Exporteer objectpunten naar DXF maakt een objecttekening. Acties > Kadastrale export DXF maakt de kadastrale uitvoer. Acties > Discipline-overzicht naar Excel gebruikt de geladen objectgegevens en de ingestelde woordenlijst."),
            ("Voortgang en uitvoer", "Een klein venster midden in het scherm toont de voortgang en de huidige stap. Annuleren blijft onderaan bereikbaar zolang stoppen nog mogelijk is. Wacht op de afrondingsmelding en lees eventuele meldingen. Houd de DXF en bijbehorende rasterbestanden samen bij verplaatsen of delen. De naambordjes op de werkkaart veranderen de exportopmaak niet."),
        ],
    },
    Topic {
        title: "Instellingen en bewaren",
        summary: "Voorkeuren, projectbestanden en automatische back-ups beheren.",
        keywords: "instellingen opslaan laden project json autosave back-up backups herstel woordenlijst kleur",
        sections: &[
            ("Instellingen per onderdeel", "Kaarten bevat de keuzes voor de werkkaart en kaartuitvoer. DXF-export bevat de tekenopties. Proefsleuven bevat profiel-, maaiveld- en logo-instellingen. KickTheMap bevat woorden, materiaalkeuzes en naamkoppelingen. Back-ups bevat automatisch opslaan en herstel."),
            ("Toepassen of annuleren", "Instellingen worden pas toegepast wanneer je op Opslaan klikt. Met Annuleren behoud je de bestaande keuzes. Als het venster meldt dat instellingen bij het project horen, sla dan daarna ook het projectbestand op."),
            ("Project bewaren en openen", "Gebruik Acties > Project opslaan of Project opslaan als. Acties > Project laden opent een eerder project. Bewaar de bronbestanden op een bereikbare plaats: het project kan naar externe TIFF- en DXF-bestanden verwijzen."),
            ("Automatisch opslaan en herstellen", "Onder Instellingen > Back-ups zet je Autosave aan en kies je het interval en het maximumaantal. Met Autosave laden open je een beschikbare eerdere versie. Gebruik Project opslaan daarnaast voor het projectbestand waarmee je verder wilt werken."),
            ("Aanvullende hulpmiddelen", "Rechtsklik op Instellingen en kies Aanvullende hulpmiddelen voor extra import- en bewerkingsopties. Ctrl+Shift+U toont of verbergt deze hulpmiddelen. De gewone menubalk blijft het uitgangspunt voor de dagelijkse workflow."),
        ],
    },
    Topic {
        title: "Over SleufBase",
        summary: "Een werkplek voor proefsleuven, kabels, leidingen en tekeningen.",
        keywords: "luka stuurman maker gemaakt ontwikkelaar versie info over help f1",
        sections: &[
            ("Gemaakt door Luka Stuurman", "SleufBase brengt KickTheMap-bestanden, MarXact-metingen, KLIC-tekeningen en virtuele proefsleuven samen. Je bekijkt en bewerkt de gegevens op de kaart en maakt uitvoer voor tekeningen en overzichten."),
            ("Deze handleiding gebruiken", "Kies een onderwerp of zoek op een functie, bijvoorbeeld naambordjes, diameter of sjabloon. Met Info naast marXact of met F1 kun je dit venster steeds opnieuw openen. De uitleg is lokaal beschikbaar en heeft geen internetverbinding nodig."),
        ],
    },
];

fn matching_topics(query: &str) -> Vec<usize> {
    let words = query.to_lowercase();
    let words: Vec<_> = words.split_whitespace().collect();
    TOPICS.iter().enumerate().filter_map(|(index, topic)| {
        if words.is_empty() {
            return Some(index);
        }
        let mut text = format!("{} {} {}", topic.title, topic.summary, topic.keywords);
        for (heading, body) in topic.sections {
            text.push_str(heading);
            text.push(' ');
            text.push_str(body);
            text.push(' ');
        }
        let text = text.to_lowercase();
        words.iter().all(|word| text.contains(*word)).then_some(index)
    }).collect()
}

/// Match the white content sections used in Instellingen.
fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1_f32, Color32::from_rgb(208, 218, 231)))
        .corner_radius(10)
        .inner_margin(16)
}

fn topic_list(ui: &mut egui::Ui, matches: &[usize], selected: &mut usize, height: f32) {
    egui::ScrollArea::vertical()
        .id_salt("sleufbase-help-topics")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The surrounding two-column row is horizontal. Always give this
            // scroll content its own vertical layout, including the topic rows.
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(ui.available_width());
                section_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.;
                    theme::strong(ui, "Onderwerpen");
                    ui.small(format!("{} onderwerpen", matches.len()));
                    ui.add_space(4.);
                    for index in matches.iter().copied() {
                        let active = *selected == index;
                        let text = egui::RichText::new(TOPICS[index].title)
                            .color(if active { Color32::WHITE } else { INK });
                        let button = egui::Button::new(text)
                            .selected(active)
                            .fill(if active { ORANGE } else { Color32::WHITE })
                            .corner_radius(6)
                            .truncate();
                        if ui.add_sized([ui.available_width(), 34.], button)
                            .on_hover_text(TOPICS[index].title).clicked()
                        {
                            *selected = index;
                        }
                    }
                });
            });
        });
}

fn topic_content(ui: &mut egui::Ui, index: usize, height: f32) {
    egui::ScrollArea::vertical()
        .id_salt(("sleufbase-help-topic", index))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_width(ui.available_width());
                let topic = &TOPICS[index];
                section_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 10.;
                    ui.heading(topic.title);
                    ui.label(egui::RichText::new(topic.summary).color(theme::MUTED));
                    ui.separator();
                    for (heading, body) in topic.sections {
                        theme::strong(ui, *heading);
                        ui.label(*body);
                        ui.add_space(6.);
                    }
                });
            });
        });
}

fn help_contents(ui: &mut egui::Ui, state: &mut State) {
    let wide = ui.available_width() >= 660.;
    ui.label(if wide {
        "Kies links een onderwerp of zoek op de naam van een functie."
    } else {
        "Kies een onderwerp of zoek op de naam van een functie."
    });
    ui.horizontal(|ui| {
        let field_width = (ui.available_width() - 64.).max(40.);
        let response = ui.add_sized(
            [field_width, 30.],
            egui::TextEdit::singleline(&mut state.query)
                .id(egui::Id::new("sleufbase-help-search"))
                .hint_text("Zoek bijvoorbeeld: naambordjes, MarXact, diameter…"),
        );
        if state.focus_search {
            response.request_focus();
            state.focus_search = false;
        }
        if ui.add_enabled(!state.query.is_empty(), egui::Button::new("Wis")).clicked() {
            state.query.clear();
            response.request_focus();
        }
    });
    ui.add_space(6.);

    let matches = matching_topics(&state.query);
    if matches.is_empty() {
        section_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::strong(ui, "Geen onderwerp gevonden");
            ui.label("Probeer een kortere zoekterm, zoals kaart, inloggen, kabel of export.");
            if ui.button("Alle onderwerpen tonen").clicked() {
                state.query.clear();
            }
        });
        return;
    }
    if !matches.contains(&state.selected) {
        state.selected = matches[0];
    }
    let height = ui.available_height().max(40.);
    if wide {
        // Reserve both columns before drawing either of them. The topic list
        // cannot consume the explanation column or shrink later buttons to
        // single-letter lines when it is nested in this horizontal row.
        let list_width = 244.;
        let gap = 16.;
        let content_width = ui.available_width() - list_width - gap;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(list_width);
                    topic_list(ui, &matches, &mut state.selected, height);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, height),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(content_width);
                    topic_content(ui, state.selected, height);
                },
            );
        });
    } else {
        egui::ComboBox::from_id_salt("sleufbase-help-topic-picker")
            .selected_text(TOPICS[state.selected].title)
            .width(ui.available_width())
            .truncate()
            .show_ui(ui, |ui| {
                for index in matches {
                    ui.selectable_value(&mut state.selected, index, TOPICS[index].title);
                }
            });
        let content_height = ui.available_height().max(40.);
        topic_content(ui, state.selected, content_height);
    }
}

pub(super) fn show(ctx: &egui::Context, state: &mut State) {
    if !state.open {
        return;
    }
    let viewport = ctx.screen_rect();
    let mut open = true;
    let mut close = false;
    egui::Window::new("Info en handleiding · SleufBase")
        .id(egui::Id::new("sleufbase-help"))
        // Use the same window geometry and standard frame as Instellingen.
        // Explicit geometry also replaces an older, incorrectly sized window.
        .fixed_pos(viewport.min + egui::vec2(12., 12.))
        .fixed_size(egui::vec2(
            (viewport.width() - 56.).max(180.),
            (viewport.height() - 100.).max(140.),
        ))
        .constrain(false)
        .open(&mut open)
        .movable(false)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            super::settings_ui::control_style(ui);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            // Match the settings footer: it remains outside both scroll areas.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal_wrapped(|ui| {
                    close = ui.button("Sluiten").clicked();
                    theme::strong(ui, "Gemaakt door Luka Stuurman");
                    ui.colored_label(theme::MUTED,
                        format!("SleufBase Rust {}", env!("CARGO_PKG_VERSION")));
                });
                ui.separator();
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    help_contents(ui, state);
                });
            });
        });
    state.open = open && !close;
}
