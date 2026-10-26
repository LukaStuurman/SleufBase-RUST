# Gesubsamplede YCbCr-TIFFs

De onafhankelijke generator schrijft 30 bestanden: raw/Deflate-strips en tegels
met 2×1, 2×2, 4×1, 4×2 en 4×4-chromablokken, vier JPEG-TIFFs en zes grotere
BigTIFF-/bytevolgorde-/oriëntatiegevallen. De productiecode gebruikt geen Python.
De tagweergave verandert uitsluitend de in-memory directory; bronbytes blijven
ongewijzigd. Dezelfde begrensde decoder verwerkt basisbeeld, zoomuitsnede en export.

Alle niet-JPEG-pixels moeten exact overeenkomen. tifffile controleert tags en
segmenten; NumPy pakt coding units onafhankelijk uit, imagecodecs decomprimeert,
en Pillow/libtiff converteert een volledig 1×1-YCbCr-beeld naar RGBA. Dit vermijdt
een aangetoonde directe libtiff-4×4-leesafwijking; dat oorspronkelijke beeld is
afzonderlijk als `pillow_rgba` bewaard. De 4×4-gevallen zijn 20×24 pixels en voldoen
aan de deelbaarheidseis uit [TIFF 6.0, pagina 92–94](https://www.itu.int/itudoc/itu-t/com16/tiff-fx/docs/tiff6.pdf).
Pillow 11.3 herkent geen big-endian BigTIFF;
de tweede oracle gebruikt daarom een classic-TIFF met bytegelijke segmenten.

De vier bestaande JPEG-codecgevallen verschillen maximaal twee kleurwaarden,
of drie voor de 2×2-tegelvariant, van Pillow/libjpeg. Tests leggen die maxima per
bestand vast. Dit is geen claim van pixelidentieke JPEG-weergave. Uitsneden moeten
wel exact dezelfde pixels opleveren als de volledige native decode. Ontbrekende
en dubbele directorytags en te grote gecomprimeerde segmenten worden afgewezen.

Reproduceren met ontwikkelafhankelijkheden Pillow 11.3.0, numpy 2.2.6,
tifffile 2025.5.10 en imagecodecs 2025.3.30:

```
python tools/generate_subsampled_tiff_fixtures.py
```

`SLEUFBASE_SUBSAMPLED_TIFF_FIXTURES` kiest een aparte uitvoermap voor bytevergelijking.
