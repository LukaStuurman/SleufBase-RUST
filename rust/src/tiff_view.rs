//! Read-only TIFF tag view for palette and WhiteIsZero decoding.
//! tiff 0.11 does not expose palette indices and cannot invert gray+alpha.
//! Present those samples as BlackIsZero to the decoder, then apply the original
//! palette/inversion ourselves. No source bytes or pixel data are rewritten.
use std::io::{self, Read, Seek, SeekFrom};

pub(crate) struct TiffView<R> {
    inner: R,
    position: u64,
    patches: Vec<(u64, Vec<u8>)>,
}
impl<R: Read + Seek> TiffView<R> {
    pub fn new(mut inner: R) -> io::Result<Self> {
        inner.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner,
            position: 0,
            patches: Vec::new(),
        })
    }
    pub fn black_is_zero(&mut self) -> io::Result<()> {self.unsigned_tags(&[(262,1)])}
    /// Replace scalar numeric tags in the first IFD, never the source file.
    /// Used to expose packed YCbCr coding units as bounded gray-byte chunks.
    pub fn unsigned_tags(&mut self,tags:&[(u16,u32)])->io::Result<()> {
        let saved=self.position;
        let result=self.locate_tags(tags);
        self.inner.seek(SeekFrom::Start(saved))?;self.position=saved;
        self.patches.extend(result?);Ok(())
    }
    fn locate_tags(&mut self,tags:&[(u16,u32)])->io::Result<Vec<(u64,Vec<u8>)>>{
        let bad=||io::Error::new(io::ErrorKind::InvalidData,"Ongeldige TIFF-tagdirectory");
        self.inner.seek(SeekFrom::Start(0))?;let mut header=[0u8;8];self.inner.read_exact(&mut header)?;
        let little=match &header[..2]{b"II"=>true,b"MM"=>false,_=>return Err(bad())};
        let u16_at=|s:&[u8]|if little{u16::from_le_bytes(s.try_into().unwrap())}else{u16::from_be_bytes(s.try_into().unwrap())};
        let u32_at=|s:&[u8]|if little{u32::from_le_bytes(s.try_into().unwrap())}else{u32::from_be_bytes(s.try_into().unwrap())};
        let u64_at=|s:&[u8]|if little{u64::from_le_bytes(s.try_into().unwrap())}else{u64::from_be_bytes(s.try_into().unwrap())};
        let big=match u16_at(&header[2..4]){42=>false,43=>true,_=>return Err(bad())};
        let offset=if big{if u16_at(&header[4..6])!=8||u16_at(&header[6..8])!=0{return Err(bad());}let mut b=[0;8];self.inner.read_exact(&mut b)?;u64_at(&b)}else{u32_at(&header[4..8])as u64};
        self.inner.seek(SeekFrom::Start(offset))?;
        let count=if big{let mut b=[0;8];self.inner.read_exact(&mut b)?;u64_at(&b)}else{let mut b=[0;2];self.inner.read_exact(&mut b)?;u16_at(&b)as u64};
        if count==0||count>65535{return Err(bad());}
        let entry_len=if big{20}else{12};let base=offset.checked_add(if big{8}else{2}).ok_or_else(bad)?;let mut patches=Vec::new();let mut seen=std::collections::HashSet::new();
        for i in 0..count{
            let at=base.checked_add(i*entry_len).ok_or_else(bad)?;self.inner.seek(SeekFrom::Start(at))?;
            let mut bytes=[0;20];self.inner.read_exact(&mut bytes[..entry_len as usize])?;
            let Some((_,value))=tags.iter().find(|(tag,_)|*tag==u16_at(&bytes[..2]))else{continue;};
            if !seen.insert(u16_at(&bytes[..2])){return Err(bad());}
            // Encode a single LONG inline. All rewritten geometry is bounded
            // before here and fits u32, including in BigTIFF directories.
            bytes[2..4].copy_from_slice(&if little{4u16.to_le_bytes()}else{4u16.to_be_bytes()});
            if big{bytes[4..12].copy_from_slice(&if little{1u64.to_le_bytes()}else{1u64.to_be_bytes()});bytes[12..20].fill(0);bytes[12..16].copy_from_slice(&if little{value.to_le_bytes()}else{value.to_be_bytes()});}
            else{bytes[4..8].copy_from_slice(&if little{1u32.to_le_bytes()}else{1u32.to_be_bytes()});bytes[8..12].copy_from_slice(&if little{value.to_le_bytes()}else{value.to_be_bytes()});}
            patches.push((at,bytes[..entry_len as usize].to_vec()));
        }
        if patches.len()!=tags.len(){return Err(bad());}Ok(patches)
    }

}
impl<R: Read> Read for TiffView<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        for (offset, bytes) in &self.patches {
            for (i, byte) in bytes.iter().copied().enumerate() {
                if let Some(index) = (offset + i as u64)
                    .checked_sub(self.position)
                    .filter(|v| *v < count as u64)
                {
                    buf[index as usize] = byte;
                }
            }
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl<R: Seek> Seek for TiffView<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.position = self.inner.seek(pos)?;
        Ok(self.position)
    }
}

#[cfg(test)]mod tests{
 use super::*;use std::io::Cursor;
 fn fixture()->Vec<u8>{std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/subsampled-tiff/ycbcr-2x2-c1-strips.tif")).unwrap()}
 fn entry(bytes:&[u8],tag:u16)->usize{let n=u16::from_le_bytes(bytes[8..10].try_into().unwrap())as usize;(0..n).map(|i|10+i*12).find(|i|u16::from_le_bytes(bytes[*i..*i+2].try_into().unwrap())==tag).unwrap()}
 #[test]fn scalar_patch_rejects_missing_and_duplicate_tags_without_touching_source(){
  let original=fixture();let mut missing=original.clone();let at=entry(&missing,258);missing[at..at+2].copy_from_slice(&65000u16.to_le_bytes());let mut view=TiffView::new(Cursor::new(missing)).unwrap();assert!(view.unsigned_tags(&[(258,8)]).is_err());
  let mut duplicate=original.clone();let at=entry(&duplicate,257);duplicate[at..at+2].copy_from_slice(&256u16.to_le_bytes());let mut view=TiffView::new(Cursor::new(duplicate)).unwrap();assert!(view.unsigned_tags(&[(256,19)]).is_err());
  let mut view=TiffView::new(Cursor::new(original.clone())).unwrap();view.unsigned_tags(&[(256,100)]).unwrap();let mut output=Vec::new();view.read_to_end(&mut output).unwrap();assert_ne!(output,original);assert_eq!(view.inner.into_inner(),original);
 }
}
