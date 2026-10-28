# SleufBase Rust – UI preview 8

## DXF-sjabloon openen in AutoCAD

- Nieuwe afbeeldingslagen, geometrielagen en Normaal/Reverse-lagen krijgen nu de plotstijl- en materiaalverwijzingen uit het sjabloon. De ontbrekende PlotStyleName-verwijzing zorgde ervoor dat AutoCAD de export afwees met “Invalid or incomplete DXF input”, gevolgd door de Enter-melding.
- Bijzondere tekens buiten het basis-Unicodebereik, zoals emoji, worden bij nieuwe tekst en geometrie als DXF-escapes voor UTF-16 geschreven. AutoCAD wees de letterlijke vierbyte-UTF-8-tekens af. Bestaande onbekende sjabloonrecords blijven ongewijzigd.
- De tekst- en uitvoerlimieten gelden ook na deze omzetting; een afgewezen tekstbewerking behoudt de eerdere inhoud.

## Controle

De fout is zonder Computer Use gereproduceerd met de geïnstalleerde AutoCAD 2026 Core Console. Die wees de export uit preview 7 af wegens een ontbrekende PlotStyleName. Vier nieuwe exportvarianten zijn vervolgens ingelezen, met AUDIT gecontroleerd en als tijdelijke DWG opgeslagen: gewone profielen, Normaal/Reverse, uitgebreide bladen en alleen afbeeldingen. Alle controles meldden nul fouten. Dit is een controle met de AutoCAD-lezer; de zichtbare AutoCAD-interface is niet bediend.

Nieuwe regressietests controleren de laagverwijzingen bij beide exportrichtingen, bijzondere tekens, tekstlimieten en het behouden van eerdere tekst bij fouten. De aanvullende lokale AutoCAD-controle weigert ook een ongeldig DXF wanneer het consoleproces zelf afsluit met code nul. Deze acceptatiecontrole kan apart worden uitgevoerd met een geïnstalleerde en gelicentieerde AutoCAD Core Console.

Maak bestaande mislukte exports opnieuw vanuit SleufBase; eerdere DXF-bestanden worden niet automatisch gewijzigd. De installer behoudt de volledige WebView2-runtime en de KickTheMap-aanpassingen uit preview 7.
