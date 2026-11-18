//! Georeferenced, annotation-free overhead raster from a PotreeConverter 2.0
//! DEFAULT octree. No WebView screenshot, survey markers or third-party tools:
 //! only decoded native XYZ/RGB/intensity are used.
use crate::{pointcloud::{self,CloudCrs,PointCloudSource},input};
use serde::Deserialize;
use std::{collections::{HashSet,VecDeque},fs::{self,File},io::{Read,Seek,SeekFrom},path::{Path,PathBuf},sync::atomic::AtomicBool};
use tiff::{encoder::{TiffEncoder,colortype::RGBA8},tags::Tag};

const METADATA_LIMIT:u64=4*1024*1024;
const HIERARCHY_LIMIT:u64=256*1024*1024;
const NODE_LIMIT:usize=1_000_000;
const MAX_CELLS:usize=8_388_608;
const MAX_VIEW_SIDE:u32=2800;

#[derive(Deserialize)]
struct Metadata {
    encoding:String,
    scale:[f64;3],
    offset:[f64;3],
    hierarchy:Hierarchy,
    attributes:Vec<Attribute>,
}
#[derive(Deserialize)]
struct Hierarchy { #[serde(rename="firstChunkSize")] first_chunk_size:u64 }
#[derive(Deserialize)]
struct Attribute { name:String, size:usize }
#[derive(Clone,Copy)]
struct Channel { offset:usize, size:usize }
struct Layout {
    width:u32,height:u32,
    min_x:f64,max_y:f64,pixel:f64,
}
pub fn path_for(id:&str)->Result<PathBuf,String> {
    Ok(pointcloud::cloud_directory(id)?.join("orthoview.tif"))
}
fn check(cancel:&AtomicBool)->Result<(),String>{input::check(cancel)}
fn u16le(data:&[u8],at:usize)->u16 {u16::from_le_bytes(data[at..at+2].try_into().unwrap())}
fn u32le(data:&[u8],at:usize)->u32 {u32::from_le_bytes(data[at..at+4].try_into().unwrap())}
fn u64le(data:&[u8],at:usize)->u64 {u64::from_le_bytes(data[at..at+8].try_into().unwrap())}
fn rgb8(value:u16)->u8{if value<=255{value as u8}else{(value>>8) as u8}}
fn bounds(source:&PointCloudSource)->Result<Layout,String>{
    let dx=(source.bounds_max[0]-source.bounds_min[0]).max(0.05);
    let dy=(source.bounds_max[1]-source.bounds_min[1]).max(0.05);
    let padding=(dx.max(dy)*0.008).clamp(0.05,2.);
    let span_x=dx+padding*2.;
    let span_y=dy+padding*2.;
    let pixel=(span_x.max(span_y)/f64::from(MAX_VIEW_SIDE)).max(0.008);
    let width=(span_x/pixel).ceil().max(2.) as u32;
    let height=(span_y/pixel).ceil().max(2.) as u32;
    let cells=(width as usize).checked_mul(height as usize).ok_or("GeoTIFF-formaat is te groot")?;
    if cells==0 || cells>MAX_CELLS {return Err("GeoTIFF-uitsnede is te groot".into());}
    Ok(Layout{width,height,min_x:source.bounds_min[0]-padding,
        max_y:source.bounds_max[1]+padding,pixel})
}
/// Read existing DEFAULT uncompressed Potree nodes. Hierarchy records are
/// 22 bytes; type 2 points to a hierarchy chunk, not an octree data block.
fn read_points(cloud:&Path,source:&PointCloudSource,layout:&Layout,
    colors:&mut [[u8;4]],heights:&mut [f32],cancel:&AtomicBool)->Result<usize,String>{
    let metadata_path=cloud.join("metadata.json");
    let info=fs::metadata(&metadata_path).map_err(|error|error.to_string())?;
    if info.len()>METADATA_LIMIT{return Err("Puntenwolkmetadata is te groot".into());}
    let meta:Metadata=serde_json::from_reader(File::open(&metadata_path).map_err(|e|e.to_string())?)
        .map_err(|e|format!("Puntenwolkmetadata lezen mislukt: {e}"))?;
    if meta.encoding!="DEFAULT" || meta.scale.iter().any(|v|!v.is_finite()||*v<=0.)
        || meta.offset.iter().any(|v|!v.is_finite()) {
        return Err("Voor dit bovenaanzicht is een lokale Potree DEFAULT-scan nodig.".into());
    }
    let mut stride=0usize;let mut position=None;let mut rgb=None;let mut intensity=None;
    for attribute in &meta.attributes {
        if attribute.size==0 || attribute.size>256 {return Err("Ongeldig puntenwolk-attribuut".into());}
        let channel=Channel{offset:stride,size:attribute.size};
        match attribute.name.to_ascii_lowercase().as_str() {
            "position"=>position=Some(channel),
            "rgb"|"rgba"|"color"=>rgb=Some(channel),
            "intensity"=>intensity=Some(channel),
            _=>{}
        }
        stride=stride.checked_add(attribute.size).ok_or("Puntenwolk-attributen zijn te groot")?;
    }
    let position=position.ok_or("Puntenwolk bevat geen XYZ-posities")?;
    if position.size!=12 || stride>2048 {return Err("Niet-ondersteunde puntenwolkpositie".into());}
    let rgb=rgb.filter(|c|c.size>=6);
    let intensity=intensity.filter(|c|c.size>=2);
    let hier_file=File::open(cloud.join("hierarchy.bin")).map_err(|e|format!("Puntenwolkhierarchie ontbreekt: {e}"))?;
    let size=hier_file.metadata().map_err(|e|e.to_string())?.len();
    if size==0||size>HIERARCHY_LIMIT{return Err("Puntenwolkhierarchie is te groot of leeg".into());}
    let mut hierarchy=Vec::with_capacity(size as usize);
    let mut hier_file=hier_file;
    hier_file.read_to_end(&mut hierarchy).map_err(|e|e.to_string())?;
    let mut octree=File::open(cloud.join("octree.bin")).map_err(|e|e.to_string())?;
    let octree_size=octree.metadata().map_err(|e|e.to_string())?.len();
    let mut queued=VecDeque::from([(0_u64,meta.hierarchy.first_chunk_size)]);
    let mut seen=HashSet::<(u64,u64)>::new();
    let mut visited=0usize;
    let mut plotted=0usize;
    let sample_every=(source.point_count/25_000_000).max(1);
    let mut sampled=0_u64;
    let mut data=vec![0_u8;stride*4096];
    while let Some((chunk_offset,chunk_size))=queued.pop_front() {
        check(cancel)?;
        if !seen.insert((chunk_offset,chunk_size)) {continue;}
        if chunk_size==0 || chunk_size%22!=0
            || chunk_offset.checked_add(chunk_size).is_none_or(|end|end>size) {
            return Err("De puntenwolk heeft een ongeldige hierarchieverwijzing".into());
        }
        for entry in hierarchy[chunk_offset as usize..(chunk_offset+chunk_size) as usize].chunks_exact(22){
            visited+=1;
            if visited>NODE_LIMIT{return Err("De puntenwolk bevat te veel hierarchieknooppunten".into());}
            let kind=entry[0];
            let count=u32le(entry,2) as usize;
            let start=u64le(entry,6);
            let bytes=u64le(entry,14);
            if kind==2 {queued.push_back((start,bytes));continue;}
            if kind>2{return Err("Niet-ondersteund Potree-knooppunttype".into());}
            if count==0 {continue;}
            let expected=count.checked_mul(stride).ok_or("Puntenwolkblok te groot")? as u64;
            if expected!=bytes || start.checked_add(bytes).is_none_or(|end|end>octree_size) {
                return Err("Het puntgegevensblok is beschadigd of onvolledig".into());
            }
            octree.seek(SeekFrom::Start(start)).map_err(|e|e.to_string())?;
            let mut remaining=count;
            while remaining>0 {
                check(cancel)?;
                let take=remaining.min(4096);
                octree.read_exact(&mut data[..take*stride]).map_err(|e|e.to_string())?;
                for point in data[..take*stride].chunks_exact(stride) {
                    sampled+=1;
                    if sampled%sample_every!=0 {continue;}
                    let pos=&point[position.offset..position.offset+12];
                    let coords=[0,1,2].map(|i| {
                        let raw=i32::from_le_bytes(pos[i*4..i*4+4].try_into().unwrap());
                        f64::from(raw)*meta.scale[i]+meta.offset[i]
                    });
                    let x=((coords[0]-layout.min_x)/layout.pixel).floor() as i64;
                    let y=((layout.max_y-coords[1])/layout.pixel).floor() as i64;
                    if x<0||y<0||x>=i64::from(layout.width)||y>=i64::from(layout.height)||!coords[2].is_finite(){continue;}
                    let index=y as usize*layout.width as usize+x as usize;
                    if coords[2] as f32 <= heights[index] {continue;}
                    let color=if let Some(c)=rgb {
                        let rgb=&point[c.offset..c.offset+6];
                        let out=[rgb8(u16le(rgb,0)),rgb8(u16le(rgb,2)),rgb8(u16le(rgb,4))];
                        if out!=[0,0,0] {out}
                        else if let Some(intensity)=intensity {
                            let gray=rgb8(u16le(point,intensity.offset));[gray;3]
                        } else {out}
                    } else if let Some(intensity)=intensity {
                        let gray=rgb8(u16le(point,intensity.offset));[gray;3]
                    } else {
                        let min=source.bounds_min[2];let max=source.bounds_max[2];
                        let normalized=if max-min>1e-6{((coords[2]-min)/(max-min)).clamp(0.,1.)}else{0.5};
                        let gray=(60.+normalized*175.) as u8;[gray;3]
                    };
                    heights[index]=coords[2] as f32;
                    colors[index]=[color[0],color[1],color[2],255];
                    plotted+=1;
                }
                remaining-=take;
            }
        }
    }
    Ok(plotted)
}
/// Fill only tiny holes in the overhead surface; no annotation markers are
/// ever rendered. Empty regions outside scan coverage remain transparent.
fn interpolate(image:&mut Vec<[u8;4]>,width:usize,height:usize,cancel:&AtomicBool)->Result<(),String>{
    for _ in 0..3 {
        check(cancel)?;
        let before=image.clone();
        for y in 1..height.saturating_sub(1) {
            if y%256==0 {check(cancel)?;}
            for x in 1..width.saturating_sub(1) {
                let at=y*width+x;
                if before[at][3]!=0 {continue;}
                let mut sum=[0_u32;3];let mut neighbors=0;
                for dy in -1_isize..=1 {for dx in -1_isize..=1 {
                    if dx==0&&dy==0{continue;}
                    let n=(y as isize+dy) as usize*width+(x as isize+dx) as usize;
                    if before[n][3]>0 {
                        for (s,c) in sum.iter_mut().zip(&before[n][..3]){*s+=u32::from(*c);}
                        neighbors+=1;
                    }
                }}
                if neighbors>=2 {
                    image[at]=[
                        (sum[0]/neighbors) as u8,(sum[1]/neighbors) as u8,
                        (sum[2]/neighbors) as u8,255,
                    ];
                }
            }
        }
    }
    Ok(())
}
/// Build an EPSG:28992 GeoTIFF from the native scan cache with no overlays.
/// Color is RGB when available, otherwise intensity or elevation grayscale.
pub fn generate(cloud:&Path,source:&PointCloudSource,cancel:&AtomicBool)->Result<PathBuf,String>{
    if source.crs!=CloudCrs::RdNew {
        return Err("Voor een GeoTIFF op de SleufBase-kaart zijn bevestigde RD-coördinaten nodig.".into());
    }
    source.validate()?;
    let layout=bounds(source)?;
    let pixels=layout.width as usize*layout.height as usize;
    let mut colors=vec![[0_u8;4];pixels];
    let mut heights=vec![f32::NEG_INFINITY;pixels];
    if read_points(cloud,source,&layout,&mut colors,&mut heights,cancel)?==0 {
        return Err("Geen scanpunten binnen de GeoTIFF-uitsnede gevonden".into());
    }
    interpolate(&mut colors,layout.width as usize,layout.height as usize,cancel)?;
    let output=cloud.join("orthoview.tif");
    let mut staged=tempfile::NamedTempFile::new_in(cloud).map_err(|e|e.to_string())?;
    {
        let mut encoder=TiffEncoder::new(staged.as_file_mut()).map_err(|e|e.to_string())?;
        let mut image=encoder.new_image::<RGBA8>(layout.width,layout.height).map_err(|e|e.to_string())?;
        image.encoder().write_tag(Tag::ModelPixelScaleTag,&[layout.pixel,layout.pixel,0.][..]).map_err(|e|e.to_string())?;
        image.encoder().write_tag(Tag::ModelTiepointTag,
            &[0.0,0.0,0.0,layout.min_x,layout.max_y,0.0][..]).map_err(|e|e.to_string())?;
        image.encoder().write_tag(Tag::GeoKeyDirectoryTag,
            &[1_u16,1,0,3,1024,0,1,1,1025,0,1,1,3072,0,1,28992][..]).map_err(|e|e.to_string())?;
        image.encoder().write_tag(Tag::ExtraSamples,&[2_u16][..]).map_err(|e|e.to_string())?;
        let flat=colors.iter().flat_map(|px|px.iter().copied()).collect::<Vec<_>>();
        image.write_data(&flat).map_err(|e|format!("GeoTIFF schrijven mislukt: {e}"))?;
    }
    staged.as_file().sync_all().map_err(|e|e.to_string())?;
    staged.persist(&output).map_err(|e|format!("GeoTIFF definitief opslaan mislukt: {e}"))?;
    Ok(output)
}
