//! Bounded strip/tile TIFF decoding and aspect-preserving map previews.
//! Source samples remain in the TIFF; only requested preview pixels become RGBA.
use crate::{
    raster_color::{Chunk, Colors},
    tiff_view::TiffView,
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tiff::{
    decoder::{ifd::Value, Decoder, Limits},
    tags::Tag,
};

pub const MAX_PREVIEW_PIXELS: u64 = 16_000_000;
pub const MAX_CHUNK_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug)]
pub struct PreviewOptions {
    pub max_side: u32,
    pub max_pixels: u64,
    pub max_chunk_bytes: usize,
}
impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            max_side: 2048,
            max_pixels: 2048 * 2048,
            max_chunk_bytes: 32 * 1024 * 1024,
        }
    }
}
impl PreviewOptions {
    fn validate(self) -> Result<(), String> {
        if self.max_side == 0
            || self.max_side > 8192
            || self.max_pixels == 0
            || self.max_pixels > MAX_PREVIEW_PIXELS
            || self.max_chunk_bytes == 0
            || self.max_chunk_bytes > MAX_CHUNK_BYTES
        {
            return Err("Ongeldige rasterpreviewlimieten".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct DecodeStats {
    pub source_pixels: u64,
    pub decoded_chunks: u32,
    pub skipped_chunks: u32,
    pub peak_chunk_bytes: usize,
}
#[derive(Debug)]
pub struct RasterPreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub stats: DecodeStats,
}

pub fn oriented_dimensions(
    width: u32,
    height: u32,
    orientation: u32,
) -> Result<(u32, u32), String> {
    if !(1..=8).contains(&orientation) {
        return Err("Ongeldige TIFF-oriëntatie".into());
    }
    Ok(if orientation >= 5 {
        (height, width)
    } else {
        (width, height)
    })
}
/// Map normalized raw GeoTIFF pixel-edge coordinates into the oriented texture.
/// Unlike pixel indices, texture edges use 1-u / 1-v (no width-minus-one).
pub fn oriented_uv([u,v]:[f64;2],orientation:u32)->Result<[f64;2],String>{
    if !u.is_finite()||!v.is_finite()||!(1..=8).contains(&orientation){return Err("Ongeldige rastertexturecoördinaten".into());}
    Ok(match orientation{2=>[1.-u,v],3=>[1.-u,1.-v],4=>[u,1.-v],5=>[v,u],6=>[1.-v,u],7=>[1.-v,1.-u],8=>[v,1.-u],_=>[u,v]})
}
fn oriented_pixel(x: u32, y: u32, w: u32, h: u32, orientation: u32) -> (u32, u32) {
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (h - 1 - y, x),
        7 => (h - 1 - y, w - 1 - x),
        8 => (y, w - 1 - x),
        _ => (x, y),
    }
}
fn preview_dimensions(w: u32, h: u32, options: PreviewOptions) -> (u32, u32) {
    let scale = (f64::from(options.max_side) / f64::from(w))
        .min(f64::from(options.max_side) / f64::from(h))
        .min((options.max_pixels as f64 / (u64::from(w) * u64::from(h)) as f64).sqrt())
        .min(1.0);
    let mut width = (f64::from(w) * scale).floor().max(1.0) as u32;
    let mut height = (f64::from(h) * scale).floor().max(1.0) as u32;
    while u64::from(width) * u64::from(height) > options.max_pixels {
        if width >= height {
            width -= 1;
        } else {
            height -= 1;
        }
    }
    (width, height)
}
fn sample_positions(source: u32, target: u32) -> Vec<u32> {
    (0..target)
        .map(|p| (((2 * u64::from(p) + 1) * u64::from(source)) / (2 * u64::from(target))) as u32)
        .collect()
}
fn selected_positions(
    full: bool,
    positions: &[u32],
    start: usize,
    end: usize,
) -> impl Iterator<Item = (usize, u32)> + '_ {
    (start..end).map(move |index| (index, if full { index as u32 } else { positions[index] }))
}
fn numbers(value: Value) -> Result<Vec<f64>, String> {
    let v = match value {
        Value::List(values) => {
            return values
                .into_iter()
                .map(numbers)
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.into_iter().flatten().collect())
        }
        Value::Rational(a, b) if b != 0 => f64::from(a) / f64::from(b),
        Value::SRational(a, b) if b != 0 => f64::from(a) / f64::from(b),
        Value::Double(v) => v,
        Value::Float(v) => f64::from(v),
        Value::Short(v) => f64::from(v),
        Value::Unsigned(v) => f64::from(v),
        _ => return Err("Ongeldige numerieke TIFF-kleurtag".into()),
    };
    if !v.is_finite() {
        return Err("Niet-eindige TIFF-kleurtag".into());
    }
    Ok(vec![v])
}
fn numeric_tag<R: Read + Seek>(
    decoder: &mut Decoder<R>,
    tag: Tag,
    default: &[f64],
) -> Result<Vec<f64>, String> {
    decoder
        .find_tag(tag)
        .map_err(|e| e.to_string())?
        .map(numbers)
        .unwrap_or_else(|| Ok(default.to_vec()))
}

/// A normalized rectangle in the already oriented full-image texture.
/// Rounded outward to source pixels so camera movement never loses an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewWindow {pub uv:[f64;4]}
#[derive(Debug)]
pub struct RasterWindow {pub preview:RasterPreview,pub uv:[f64;4]}
pub fn decode_window_with_cancel(path:&Path,options:PreviewOptions,window:PreviewWindow,cancel:&AtomicBool)->Result<RasterWindow,String>{
    decode_window_pixels(path,options,false,Some(window),cancel)
}
fn window_pixels(window:Option<PreviewWindow>,w:u32,h:u32,orientation:u32)->Result<([u32;4],[f64;4]),String>{
    let(ow,oh)=oriented_dimensions(w,h,orientation)?;
    let uv=window.map_or([0.,0.,1.,1.],|v|v.uv);
    if uv.iter().any(|v|!v.is_finite())||uv[0]>=uv[2]||uv[1]>=uv[3]||uv[2]<=0.||uv[3]<=0.||uv[0]>=1.||uv[1]>=1.{return Err("Ongeldig zichtbaar rastergebied".into());}
    let left=(uv[0].clamp(0.,1.)*ow as f64).floor()as u32;let top=(uv[1].clamp(0.,1.)*oh as f64).floor()as u32;
    let right=(uv[2].clamp(0.,1.)*ow as f64).ceil()as u32;let bottom=(uv[3].clamp(0.,1.)*oh as f64).ceil()as u32;
    let inverse=match orientation{6=>8,8=>6,n=>n};
    let points=[(left,top),(right-1,top),(left,bottom-1),(right-1,bottom-1)].map(|(x,y)|oriented_pixel(x,y,ow,oh,inverse));
    let raw=[points.iter().map(|p|p.0).min().unwrap(),points.iter().map(|p|p.1).min().unwrap(),points.iter().map(|p|p.0).max().unwrap()+1,points.iter().map(|p|p.1).max().unwrap()+1];
    Ok((raw,[left as f64/ow as f64,top as f64/oh as f64,right as f64/ow as f64,bottom as f64/oh as f64]))
}
pub fn decode_tiff_preview(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let preview = decode_preview(path, PreviewOptions::default())?;
    Ok((preview.width, preview.height, preview.rgba))
}
pub fn decode_preview(path: &Path, options: PreviewOptions) -> Result<RasterPreview, String> {
    decode_preview_with_cancel(path, options, &AtomicBool::new(false))
}
pub fn decode_preview_with_cancel(
    path: &Path,
    options: PreviewOptions,
    cancel: &AtomicBool,
) -> Result<RasterPreview, String> {
    decode_pixels(path, options, false, cancel)
}
/// Full export pixels, with the same source/chunk safety limits and color
/// conversion as previews. Never silently reduce export resolution.
pub fn decode_full_with_cancel(path: &Path, cancel: &AtomicBool) -> Result<RasterPreview, String> {
    decode_pixels(
        path,
        PreviewOptions {
            max_chunk_bytes: MAX_CHUNK_BYTES,
            ..PreviewOptions::default()
        },
        true,
        cancel,
    )
}
fn decode_pixels(
    path: &Path,
    options: PreviewOptions,
    full: bool,
    cancel: &AtomicBool,
) -> Result<RasterPreview, String> {
    decode_window_pixels(path,options,full,None,cancel).map(|v|v.preview)
}
fn decode_window_pixels(path:&Path,options:PreviewOptions,full:bool,window:Option<PreviewWindow>,cancel:&AtomicBool)->Result<RasterWindow,String>{
    options.validate()?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Rasterverwerking geannuleerd".into());
    }
    let file = File::open(path).map_err(|e| format!("Rasterbestand openen mislukt: {e}"))?;
    let reader = TiffView::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = options.max_chunk_bytes;
    limits.intermediate_buffer_size = options.max_chunk_bytes;
    let mut decoder = Decoder::new(reader)
        .map_err(|e| format!("Rasterbestand ongeldig: {e}"))?
        .with_limits(limits);
    let (source_w, source_h) = decoder.dimensions().map_err(|e| e.to_string())?;
    let pixels = u64::from(source_w) * u64::from(source_h);
    if pixels == 0 || pixels > crate::geotiff::MAX_GEOTIFF_PIXELS {
        return Err("TIFF-afmetingen zijn leeg of overschrijden de bronlimiet".into());
    }
    let orientation = decoder
        .find_tag_unsigned::<u32>(Tag::Orientation)
        .map_err(|e| e.to_string())?
        .unwrap_or(1);
    oriented_dimensions(source_w, source_h, orientation)?;
    let photometric = decoder
        .get_tag_u32(Tag::PhotometricInterpretation)
        .map_err(|e| e.to_string())?;
    if ![0, 1, 2, 3, 5, 6].contains(&photometric) {
        return Err(format!(
            "TIFF-kleurruimte {photometric} wordt nog niet weergegeven"
        ));
    }
    let source_channels = decoder
        .find_tag_unsigned::<u32>(Tag::SamplesPerPixel)
        .map_err(|e| e.to_string())?
        .unwrap_or(1);
    let planar = decoder
        .find_tag_unsigned::<u32>(Tag::PlanarConfiguration)
        .map_err(|e| e.to_string())?
        .unwrap_or(1)
        == 2;
    let extras = decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let nodata = decoder
        .find_tag(Tag::Unknown(42113))
        .map_err(|e| e.to_string())?
        .and_then(|v| v.into_string().ok())
        .and_then(|s| s.trim_end_matches('\0').trim().parse::<f64>().ok());
    // Re-read only the IFD through an in-memory tag view, keeping the same file
    // handle. Palette indices and gray+alpha are otherwise unsupported by tiff.
    if photometric == 0 || photometric == 3 {
        decoder.inner().black_is_zero().map_err(|e| e.to_string())?;
        decoder.seek_to_image(0).map_err(|e| e.to_string())?;
    }
    let color = decoder.colortype().map_err(|e| e.to_string())?;
    let bits = color.bit_depth();
    if ![1, 2, 4, 8, 16, 32, 64].contains(&bits) {
        return Err(format!("TIFF bitdiepte {bits} wordt nog niet weergegeven"));
    }
    let channels = usize::from(color.num_samples());
    let base = match photometric {
        0 | 1 | 3 => 1,
        2 | 6 => 3,
        5 => 4,
        _ => unreachable!(),
    };
    if channels < base || channels > 1024 {
        return Err("Ongeldig TIFF-kanaalaantal".into());
    }
    let alpha = (channels > base && matches!(extras.first(), Some(1 | 2))).then_some(base);
    let palette = if photometric == 3 {
        if bits > 16 {
            return Err("TIFF-kleurenkaart heeft meer dan 16 indexbits".into());
        }
        let palette = decoder
            .get_tag_u16_vec(Tag::ColorMap)
            .map_err(|e| e.to_string())?;
        if palette.len() != 3 * (1usize << bits) {
            return Err("TIFF-kleurenkaart heeft ongeldige afmetingen".into());
        }
        palette
    } else {
        Vec::new()
    };
    let (coefficients, reference) = if photometric == 6 {
        let maximum = if bits == 64 {
            u64::MAX as f64
        } else {
            ((1u64 << bits) - 1) as f64
        };
        let middle = (1u64 << (bits - 1)) as f64;
        (
            numeric_tag(&mut decoder, Tag::Unknown(529), &[0.299, 0.587, 0.114])?,
            numeric_tag(
                &mut decoder,
                Tag::Unknown(532),
                &[0.0, maximum, middle, maximum, middle, maximum],
            )?,
        )
    } else {
        (
            vec![0.299, 0.587, 0.114],
            vec![0.0, 255.0, 128.0, 255.0, 128.0, 255.0],
        )
    };
    if coefficients.len() != 3
        || reference.len() != 6
        || coefficients[1] <= 0.0
        || reference.as_chunks::<2>().0.iter().any(|p| p[1] <= p[0])
    {
        return Err("Ongeldige TIFF YCbCr-coëfficiënten/referentie".into());
    }
    let compression=decoder.find_tag_unsigned::<u32>(Tag::Compression).map_err(|e|e.to_string())?.unwrap_or(1);
    let subsampling=decoder.find_tag_unsigned_vec::<u16>(Tag::ChromaSubsampling).map_err(|e|e.to_string())?.unwrap_or_else(||vec![2,2]);
    let packed_subsampling=if photometric==6&&compression!=7&&subsampling!=[1,1]{
        let [horizontal,vertical]=subsampling.as_slice()else{return Err("Ongeldige TIFF-chromasubsampling".into());};
        if ![1,2,4].contains(horizontal)||![1,2,4].contains(vertical)||bits!=8||planar||channels!=3||!extras.is_empty(){return Err("Ongeldige gepakte TIFF YCbCr-indeling".into());}
        Some((*horizontal as usize,*vertical as usize))
    }else{None};
    let colors = Colors {
        photometric,
        bits,
        channels,
        planar,
        alpha,
        associated: extras.first() == Some(&1),
        palette,
        nodata,
        packed_subsampling,
        ycbcr_coefficients: coefficients.try_into().unwrap(),
        ycbcr_reference: reference.try_into().unwrap(),
    };
    let (chunk_w, chunk_h) = decoder.chunk_dimensions();
    if chunk_w == 0 || chunk_h == 0 {
        return Err("TIFF heeft lege stroken/tegels".into());
    }
    let columns = source_w.div_ceil(chunk_w);
    let rows = source_h.div_ceil(chunk_h);
    let per_plane = columns.checked_mul(rows).ok_or("Te veel TIFF-tegels")?;
    let source_planes = if planar { source_channels } else { 1 };
    let tag = match decoder.get_chunk_type() {
        tiff::decoder::ChunkType::Strip => Tag::StripOffsets,
        tiff::decoder::ChunkType::Tile => Tag::TileOffsets,
    };
    // strip_count() in tiff 0.11 can overflow on valid RowsPerStrip=UINT32_MAX.
    let total_chunks = decoder
        .get_tag_u64_vec(tag)
        .map_err(|e| e.to_string())?
        .len();
    let byte_counts=decoder.get_tag_u64_vec(if tag==Tag::StripOffsets{Tag::StripByteCounts}else{Tag::TileByteCounts}).map_err(|e|e.to_string())?;
    if byte_counts.len()!=total_chunks{return Err("TIFF-segmentgrootten kloppen niet".into());}
    if per_plane.checked_mul(source_planes).map(|v| v as usize) != Some(total_chunks) {
        return Err("TIFF strook-/tegelaantal klopt niet".into());
    }
    let encoded_h = if tag == Tag::StripOffsets {
        chunk_h.min(source_h)
    } else {
        chunk_h
    };
    if let Some((horizontal,vertical))=packed_subsampling{
        let horizontal=horizontal as u32;let vertical=vertical as u32;
        if chunk_h<source_h&&chunk_h%vertical!=0{return Err("YCbCr-strookgrens doorsnijdt een chromablok".into());}
        let bytes_per_unit=horizontal*vertical+2;
        let packed_width=|width:u32|width.div_ceil(horizontal).checked_mul(bytes_per_unit).ok_or("TIFF-chromarij is te groot");
        let mut tags=vec![(256,packed_width(source_w)?),(257,source_h.div_ceil(vertical)),(258,8),(262,1),(277,1)];
        if tag==Tag::StripOffsets{tags.push((278,chunk_h.min(source_h).div_ceil(vertical)));}
        else{if chunk_w%horizontal!=0||chunk_h%vertical!=0{return Err("YCbCr-tegelgrens doorsnijdt een chromablok".into());}tags.extend([(322,packed_width(chunk_w)?),(323,chunk_h.div_ceil(vertical))]);}
        decoder.inner().unsigned_tags(&tags).map_err(|e|e.to_string())?;decoder.seek_to_image(0).map_err(|e|e.to_string())?;
    }
    let encoded_bytes = (u128::from(chunk_w)
        * u128::from(encoded_h)
        * u128::from(if planar { 1 } else { source_channels })
        * u128::from(bits))
    .div_ceil(8);
    if encoded_bytes > options.max_chunk_bytes as u128 {
        return Err("TIFF-strook/tegel overschrijdt het decodergeheugenbudget".into());
    }
    let (region,uv)=window_pixels(window,source_w,source_h,orientation)?;
    let region_w=region[2]-region[0];let region_h=region[3]-region[1];
    let (preview_w, preview_h) = if full {(source_w,source_h)}else{preview_dimensions(region_w,region_h,options)};
    let (width,height)=oriented_dimensions(preview_w,preview_h,orientation)?;
    let mut rgba=vec![0;(u64::from(width)*u64::from(height)*4)as usize];
    let xs=if full{Vec::new()}else{sample_positions(region_w,preview_w).into_iter().map(|v|v+region[0]).collect()};
    let ys=if full{Vec::new()}else{sample_positions(region_h,preview_h).into_iter().map(|v|v+region[1]).collect()};
    let mut stats = DecodeStats {
        source_pixels: pixels,
        ..Default::default()
    };
    for row in 0..rows {
        let top = row * chunk_h;
        let bottom = top.saturating_add(chunk_h).min(source_h);
        let ya = if full {
            top as usize
        } else {
            ys.partition_point(|y| *y < top)
        };
        let yb = if full {
            bottom as usize
        } else {
            ys.partition_point(|y| *y < bottom)
        };
        if ya == yb {
            continue;
        }
        for column in 0..columns {
            if cancel.load(Ordering::Relaxed) {
                return Err("Rasterverwerking geannuleerd".into());
            }
            let left = column * chunk_w;
            let right = left.saturating_add(chunk_w).min(source_w);
            let xa = if full {
                left as usize
            } else {
                xs.partition_point(|x| *x < left)
            };
            let xb = if full {
                right as usize
            } else {
                xs.partition_point(|x| *x < right)
            };
            if xa == xb {
                continue;
            }
            let mut chunks = Vec::with_capacity(colors.required_planes());
            let mut used = 0usize;
            for plane in 0..colors.required_planes() {
                let index = row * columns + column + plane as u32 * per_plane;
                // JPEG's codec first materializes its compressed payload; cap
                // that input as well as the decoded output, for every codec.
                if byte_counts[index as usize]>options.max_chunk_bytes as u64{return Err("TIFF-gecomprimeerd segment overschrijdt het geheugenbudget".into());}
                let layout = decoder
                    .image_chunk_buffer_layout(index)
                    .map_err(|e| e.to_string())?;
                used = used
                    .checked_add(layout.len)
                    .ok_or("TIFF-tegel is te groot")?;
                if used > options.max_chunk_bytes {
                    return Err("TIFF-tegelvlakken overschrijden het geheugenbudget".into());
                }
                let data = decoder
                    .read_chunk(index)
                    .map_err(|e| format!("TIFF-strook/tegel: {e}"))?;
                chunks.push(Chunk {
                    data,
                    row_stride: layout.row_stride.ok_or("TIFF mist rij-indeling")?.get(),
                });
                stats.decoded_chunks += 1;
            }
            stats.peak_chunk_bytes = stats.peak_chunk_bytes.max(used);
            // The dominant aerial-imagery path needs no numeric sample dispatch,
            // floating-point NoData checks, or general color-space conversion.
            let rgb8 = if colors.photometric == 2
                && colors.bits == 8
                && !colors.planar
                && !colors.associated
                && colors.nodata.is_none()
            {
                if let tiff::decoder::DecodingResult::U8(bytes) = &chunks[0].data {
                    Some(bytes.as_slice())
                } else {
                    None
                }
            } else {
                None
            };
            for (y, sy) in selected_positions(full, &ys, ya, yb) {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Rasterverwerking geannuleerd".into());
                }
                for (x, sx) in selected_positions(full, &xs, xa, xb) {
                    let pixel = if let Some(bytes) = rgb8 {
                        let offset = (sy - top) as usize * chunks[0].row_stride
                            + (sx - left) as usize * colors.channels;
                        let source = bytes
                            .get(offset..offset + colors.channels)
                            .ok_or("TIFF RGB-pixels zijn onvolledig")?;
                        [
                            source[0],
                            source[1],
                            source[2],
                            colors.alpha.map_or(255, |a| source[a]),
                        ]
                    } else {
                        colors.rgba(&chunks, (sx - left) as usize, (sy - top) as usize)?
                    };
                    let (dx, dy) =
                        oriented_pixel(x as u32, y as u32, preview_w, preview_h, orientation);
                    let dest = (dy as usize * width as usize + dx as usize) * 4;
                    rgba[dest..dest + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    stats.skipped_chunks = total_chunks as u32 - stats.decoded_chunks;
    Ok(RasterWindow{preview:RasterPreview {width,height,rgba,stats},uv})
}

#[cfg(test)] mod window_tests {
    use super::*;
    fn fixture(name:&str)->std::path::PathBuf{Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters").join(name)}
    #[test] fn visible_pixels_equal_full_oriented_source_for_every_orientation_and_alpha(){
        for name in (1..=8).map(|n|format!("orientation{n}.tiff")).chain(["rgba8-unassociated.tiff","rgba8-associated.tiff","palette8.tiff","rgb8-planar-tile-edges.tiff"].map(str::to_owned)){
            let path=fixture(&name);let cancel=AtomicBool::new(false);let full=decode_full_with_cancel(&path,&cancel).unwrap();
            for uv in [[0.,0.,0.5,1.],[0.25,0.2,0.9,0.85],[0.,0.,1.,1.]]{
                let window=decode_window_with_cancel(&path,PreviewOptions::default(),PreviewWindow{uv},&cancel).unwrap();
                let left=(window.uv[0]*full.width as f64).round()as u32;let top=(window.uv[1]*full.height as f64).round()as u32;
                let right=(window.uv[2]*full.width as f64).round()as u32;let bottom=(window.uv[3]*full.height as f64).round()as u32;
                assert_eq!((window.preview.width,window.preview.height),(right-left,bottom-top),"{name}");
                let expected=(top..bottom).flat_map(|y|{let a=((y*full.width+left)*4)as usize;let b=((y*full.width+right)*4)as usize;full.rgba[a..b].iter().copied()}).collect::<Vec<_>>();
                assert_eq!(window.preview.rgba,expected,"{name} {uv:?}");
            }
        }
    }
    #[test] fn crop_skips_source_chunks_and_recovers_zoom_detail(){
        let path=fixture("large-striped.tiff");let cancel=AtomicBool::new(false);let options=PreviewOptions{max_side:256,max_pixels:65536,..Default::default()};
        let overview=decode_preview_with_cancel(&path,options,&cancel).unwrap();
        let detail=decode_window_with_cancel(&path,options,PreviewWindow{uv:[0.4,0.4,0.42,0.42]},&cancel).unwrap();
        assert!(detail.preview.stats.decoded_chunks<overview.stats.decoded_chunks);assert!(detail.preview.stats.skipped_chunks>0);
        assert!(detail.preview.stats.peak_chunk_bytes<=options.max_chunk_bytes);
        let full=decode_full_with_cancel(&path,&cancel).unwrap();let x=(detail.uv[0]*full.width as f64).round()as u32;let y=(detail.uv[1]*full.height as f64).round()as u32;
        assert_eq!(&detail.preview.rgba[..4],&full.rgba[((y*full.width+x)*4)as usize..((y*full.width+x)*4+4)as usize]);
    }
}

#[cfg(test)]mod subsampled_tests{
 use super::*;
 #[test]fn compressed_segment_bound_precedes_codec_allocation(){
 let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/subsampled-tiff/ycbcr-2x2-c7-strips.tif");let mut bytes=std::fs::read(path).unwrap();let count=u16::from_le_bytes(bytes[8..10].try_into().unwrap())as usize;
 let at=(0..count).map(|i|10+i*12).find(|i|u16::from_le_bytes(bytes[*i..*i+2].try_into().unwrap())==279).unwrap();let n=u32::from_le_bytes(bytes[at+4..at+8].try_into().unwrap());let location=if n==1{at+8}else{u32::from_le_bytes(bytes[at+8..at+12].try_into().unwrap())as usize};bytes[location..location+4].copy_from_slice(&(64u32*1024*1024).to_le_bytes());
 let file=tempfile::NamedTempFile::new().unwrap();std::fs::write(file.path(),bytes).unwrap();let error=decode_preview(file.path(),PreviewOptions::default()).unwrap_err();assert!(error.contains("gecomprimeerd segment"),"{error}");
 }
 #[test]fn packed_ycbcr_and_jpeg_match_independent_pillow_pixels(){
 let cases:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/subsampled-tiff/reference.json")).unwrap();let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/subsampled-tiff");
 let mut errors=Vec::new();for c in cases.as_array().unwrap(){let file=c["file"].as_str().unwrap();let decoded=decode_full_with_cancel(&root.join(file),&AtomicBool::new(false)).unwrap_or_else(|e|panic!("{file}: {e}"));let reference=c["rgba"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()as u8).collect::<Vec<_>>();assert_eq!((decoded.width,decoded.height),(c["width"].as_u64().unwrap()as u32,c["height"].as_u64().unwrap()as u32));
 let max=decoded.rgba.iter().zip(&reference).map(|(a,b)|a.abs_diff(*b)).max().unwrap();println!("{file}: maximum color difference={max}");let allowed=if c["compression"]==7{if c["subsampling"]==serde_json::json!([2,2])&&c["tiled"]==true{3}else{2}}else{0};if max>allowed{let i=decoded.rgba.iter().zip(&reference).position(|(a,b)|a.abs_diff(*b)>allowed).unwrap_or(0)/4;println!("first mismatch pixel ({},{}): {:?} vs {:?}",i%decoded.width as usize,i/decoded.width as usize,&decoded.rgba[i*4..i*4+4],&reference[i*4..i*4+4]);errors.push(file.to_owned());}
 let crop=decode_window_with_cancel(&root.join(file),PreviewOptions::default(),PreviewWindow{uv:[0.21,0.19,0.83,0.88]},&AtomicBool::new(false)).unwrap();let x=(crop.uv[0]*decoded.width as f64).round()as u32;let y=(crop.uv[1]*decoded.height as f64).round()as u32;
 for row in 0..crop.preview.height{let a=(((row+y)*decoded.width+x)*4)as usize;let b=(row*crop.preview.width*4)as usize;assert_eq!(&crop.preview.rgba[b..b+(crop.preview.width*4)as usize],&decoded.rgba[a..a+(crop.preview.width*4)as usize],"{file}: crop");}
 let limits=PreviewOptions{max_side:8,max_pixels:64,max_chunk_bytes:4,..Default::default()};assert!(decode_preview(&root.join(file),limits).is_err());
 }
 assert!(errors.is_empty(),"pixel mismatches: {errors:?}");
 }
}
