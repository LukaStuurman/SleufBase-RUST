//! Same-document entity copies: independent owned dictionaries/linked entities,
//! shared named resources, no copied reactors, fresh image reactors.
use crate::{
    dxf_document::{Change, Document, RecordView},
    input,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    sync::atomic::AtomicBool,
};
pub(crate) type Tags = Vec<(i32, String)>;
pub(crate) fn tags(r: RecordView<'_>) -> Tags {
    r.tags().map(|(c, v)| (c, v.into())).collect()
}
fn mtext_columns(r:RecordView<'_>)->Result<Vec<u64>,String>{
    if r.kind()!="MTEXT"{return Ok(Vec::new());}
    let mut acad=false;let mut columns=false;let mut out=Vec::new();
    for(c,v)in r.tags(){
        if c==1001 {if columns{return Err("Onvolledige MTEXT-kolomverwijzingen".into());}acad=v=="ACAD";}
        else if acad&&c==1000&&v=="ACAD_MTEXT_COLUMNS_BEGIN"{if columns{return Err("Geneste MTEXT-kolomverwijzingen".into());}columns=true;}
        else if acad&&c==1000&&v=="ACAD_MTEXT_COLUMNS_END"{columns=false;}
        else if columns&&c==1005{if out.len()>=128{return Err("Te veel gekoppelde MTEXT-kolommen".into());}out.push(id(v)?);}
    }
    if columns{return Err("Onvolledige MTEXT-kolomverwijzingen".into());}Ok(out)
}
pub(crate) fn encode(tags: &Tags) -> String {
    let mut text = String::new();
    for (c, v) in tags {
        writeln!(text, "{c}\n{v}").unwrap();
    }
    text
}
pub(crate) fn first(tags: &Tags, code: i32) -> Option<&str> {
    let mut depth = 0;
    for (c, v) in tags {
        if *c == 101 || *c >= 1000 {
            break;
        }
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
            continue;
        }
        if depth == 0 && *c == code {
            return Some(v);
        }
    }
    None
}
pub(crate) fn id(value: &str) -> Result<u64, String> {
    u64::from_str_radix(value.trim(), 16).map_err(|_| "Ongeldige kopieerhandle".into())
}
pub(crate) fn owner(r: RecordView<'_>) -> Option<u64> {
    let mut depth = 0;
    for (c, v) in r.tags().take_while(|(c, _)| *c != 100) {
        if c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
        }
        if c == 330 && depth == 0 {
            return id(v).ok();
        }
    }
    None
}
pub(crate) fn replace(tags: &mut Tags, code: i32, value: String) {
    if let Some((_, v)) = tags.iter_mut().find(|(c, _)| *c == code) {
        *v = value;
    } else {
        tags.push((code, value));
    }
}
fn clean(source: Tags) -> Tags {
    let mut result = Vec::new();
    let mut skip = 0;
    for (c, v) in source {
        if skip > 0 {
            if c == 102 {
                if v.starts_with('{') {
                    skip += 1;
                } else if v == "}" {
                    skip -= 1;
                }
            }
            continue;
        }
        if c == 102 && v == "{ACAD_REACTORS" {
            skip = 1;
            continue;
        }
        result.push((c, v));
    }
    result
}
pub(crate) fn set_owner(tags: &mut Tags, value: u64) {
    let mut depth = 0;
    let mut at = 1;
    for (i, (c, v)) in tags.iter_mut().enumerate() {
        if *c == 100 {
            at = i;
            break;
        }
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
        }
        if *c == 330 && depth == 0 {
            *v = format!("{value:X}");
            return;
        }
    }
    tags.insert(at, (330, format!("{value:X}")));
}
pub(crate) fn scope_set(
    tags: &mut Tags,
    subclass: &str,
    code: i32,
    value: String,
) -> Result<(), String> {
    let start = tags
        .iter()
        .position(|(c, v)| *c == 100 && v == subclass)
        .ok_or_else(|| {
            format!(
                "Ontbrekende DXF-kopieersubklasse {subclass} in {}",
                first(tags, 0).unwrap_or("record")
            )
        })?
        + 1;
    let end = tags[start..]
        .iter()
        .position(|(c, _)| matches!(*c, 100 | 101) || *c >= 1000)
        .map_or(tags.len(), |i| i + start);
    if let Some((_, v)) = tags[start..end].iter_mut().find(|(c, _)| *c == code) {
        *v = value;
    } else {
        tags.insert(end, (code, value));
    }
    Ok(())
}
pub(crate) struct Copies<'a> {
    doc: &'a Document,
    pub next: u64,
    pub objects: String,
    pub definition_reactors: HashMap<u64, Vec<u64>>,
    count: usize,
    block_records: String,
    blocks: String,
    block_count: usize,
    dimension_stack: HashSet<u64>,
    block_names: HashSet<String>,
    linked_columns: HashSet<u64>,
    old_mtext_columns:bool,
    cancel: &'a AtomicBool,
}
impl<'a> Copies<'a> {
    pub fn cancel(&self) -> &'a AtomicBool {
        self.cancel
    }
    pub fn new(doc: &'a Document, next: u64, cancel: &'a AtomicBool) -> Self {
        let mut version_tag=false;let mut old_mtext_columns=false;
        for r in doc.records().filter(|r|r.section()==Some("HEADER")){for(c,v)in r.tags(){if c==9{version_tag=v=="$ACADVER";}else if version_tag&&c==1{old_mtext_columns=v!="AC1032";version_tag=false;}}}
        let graph=if old_mtext_columns{doc.records().filter_map(|r|Some((r.handle()?,mtext_columns(r).ok()?))).filter(|(_,v)|!v.is_empty()).collect::<HashMap<_,_>>()}else{HashMap::new()};
        // A malformed cyclic parent must stay visible to the copying pass so
        // column_group rejects it; skipping every node would silently lose it.
        let mut cyclic=HashSet::new();
        for &start in graph.keys(){let mut pending=vec![(start,0usize)];let mut seen=HashSet::new();while let Some((node,depth))=pending.pop(){if depth>16{cyclic.insert(start);break;}if !seen.insert(node){continue;}if let Some(children)=graph.get(&node){for &child in children{if child==start{cyclic.insert(start);}else{pending.push((child,depth+1));}}}}}
        let linked_columns=graph.values().flatten().copied().filter(|h|!cyclic.contains(h)).collect();
        Self {
            doc,
            next,
            objects: String::new(),
            definition_reactors: HashMap::new(),
            count: 0,
            block_records:String::new(),blocks:String::new(),block_count:0,
            dimension_stack:HashSet::new(),
            block_names:doc.records().filter(|r|r.kind()=="BLOCK_RECORD").filter_map(|r|r.first(2)).map(crate::dxf_symbols::key).collect(),
            linked_columns,
            old_mtext_columns,
            cancel,
        }
    }
    pub fn allocate(&mut self) -> Result<u64, String> {
        input::check(self.cancel)?;
        self.count += 1;
        if self.count > 100_000 {
            return Err("Te veel gekopieerde DXF-records".into());
        }
        let value = self.next;
        self.next = self.next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        Ok(value)
    }
    pub fn is_linked_column(&self,r:RecordView<'_>)->bool{r.handle().is_some_and(|h|self.linked_columns.contains(&h))}
    /// MTEXT columns are linked entities with the layout's owner, rather than
    /// INSERT-style children. Copy them in their parent group, not as roots.
    pub fn entity_group(&mut self,r:RecordView<'a>,target_owner:u64)->Result<Vec<Tags>,String>{
        self.column_group(r,target_owner,&mut HashSet::new(),0)
    }
    fn column_group(&mut self,r:RecordView<'a>,target_owner:u64,visiting:&mut HashSet<u64>,depth:usize)->Result<Vec<Tags>,String>{
        let columns=if self.old_mtext_columns{mtext_columns(r)?}else{Vec::new()};
        let source=r.handle().ok_or("DXF-kopie mist handle")?;
        if depth>16||!visiting.insert(source){return Err("Cyclische of te diepe MTEXT-kolomstructuur".into());}
        let(_,mut root)=self.entity(r,target_owner)?;let mut children=Vec::new();let mut handles=Vec::new();let mut seen=HashSet::new();
        for handle in columns{input::check(self.cancel)?;
            if !seen.insert(handle){return Err("Dubbele MTEXT-kolomverwijzing".into());}
            let child=self.doc.by_handle(&format!("{handle:X}"))?.ok_or("Gekoppelde MTEXT-kolom ontbreekt")?;
            if child.kind()!="MTEXT"||child.section()!=r.section()||owner(child).is_some_and(|o|o!=0&&Some(o)!=owner(r)){return Err("MTEXT-kolom heeft ongeldige ruimte of eigenaar".into());}
            let copied=self.column_group(child,target_owner,visiting,depth+1)?;
            handles.push(first(&copied[0],5).ok_or("Kolomkopie mist handle")?.to_owned());children.extend(copied);
        }
        let mut acad=false;let mut active=false;let mut index=0;
        if self.old_mtext_columns{for(c,v)in &mut root{if *c==1001{acad=v=="ACAD";}else if acad&&*c==1000&&v=="ACAD_MTEXT_COLUMNS_BEGIN"{active=true;}else if acad&&*c==1000&&v=="ACAD_MTEXT_COLUMNS_END"{active=false;}else if active&&*c==1005{*v=handles[index].clone();index+=1;}}}
        visiting.remove(&source);let mut result=vec![root];result.extend(children);Ok(result)
    }
    fn xdict(
        &mut self,
        old: u64,
        new_owner: u64,
        visiting: &mut HashSet<u64>,
        depth: usize,
    ) -> Result<u64, String> {
        if depth > 16 || !visiting.insert(old) {
            return Err("Cyclische of te diepe uitbreidingsdictionary".into());
        }
        let r = self
            .doc
            .by_handle(&format!("{old:X}"))?
            .ok_or("Ontbrekend eigendom in uitbreidingsdictionary")?;
        if r.section() != Some("OBJECTS") {
            return Err("Uitbreidingsdictionary verwijst buiten OBJECTS".into());
        }
        let kind = r.kind();
        if !matches!(
            kind,
            "DICTIONARY" | "XRECORD" | "DICTIONARYVAR" | "ACDBDICTIONARYWDFLT" | "SPATIAL_FILTER"
        ) {
            return Err(format!(
                "Kopiëren van dictionary-object {kind} is nog niet ondersteund"
            ));
        }
        let mut copied = clean(tags(r));
        let handle = self.allocate()?;
        replace(&mut copied, 5, format!("{handle:X}"));
        set_owner(&mut copied, new_owner);
        // Inverted XCLIP stores its round-trip paths in the filter object's
        // own extension dictionary. That graph belongs to the new filter too.
        let mut extension = false;
        for (c,v) in &mut copied {
            if *c==102 {extension=v=="{ACAD_XDICTIONARY";}
            else if extension && *c==360 {
                let child=id(v)?;
                let record=self.doc.by_handle(&format!("{child:X}"))?.ok_or("Uitbreidingsdictionary ontbreekt")?;
                if owner(record)!=Some(old){return Err("Uitbreidingsdictionary heeft ongeldig eigendom".into());}
                *v=format!("{:X}",self.xdict(child,handle,visiting,depth+1)?);
            }
        }
        // A soft-owned dictionary's entries still have one DXF owner. Sharing
        // their old objects makes a copied dictionary invalid: audit removes
        // its keys. Copy by actual ownership, retaining the original flag.
        if matches!(kind, "DICTIONARY" | "ACDBDICTIONARYWDFLT") {
            let mut appdata=0usize;
            for (c, v) in &mut copied {
                if *c==102 {if v.starts_with('{'){appdata+=1;}else if v=="}"{appdata=appdata.saturating_sub(1);}continue;}
                if appdata>0 {continue;}
                if matches!(*c, 350 | 360) {
                    let child = id(v)?;
                    if child != 0 {
                        let record = self
                            .doc
                            .by_handle(&format!("{child:X}"))?
                            .ok_or("Dictionarywaarde ontbreekt")?;
                        if owner(record) != Some(old) {
                            return Err("Dictionarywaarde heeft ongeldig eigendom".into());
                        }
                        *v = format!("{:X}", self.xdict(child, handle, visiting, depth + 1)?);
                    }
                }
            }
        }
        self.objects.push_str(&encode(&copied));
        visiting.remove(&old);
        Ok(handle)
    }
    pub fn entity(&mut self, r: RecordView<'a>, owner: u64) -> Result<(u64, Tags), String> {
        if matches!(
            r.kind(),
            "ACAD_PROXY_ENTITY"
                | "ACAD_TABLE"
        ) {
            return Err(format!(
                "Zelfstandig kopiëren van {} wordt nog niet ondersteund",
                r.kind()
            ));
        }
        let handle = self.allocate()?;
        let mut copied = clean(tags(r));
        replace(&mut copied, 5, format!("{handle:X}"));
        set_owner(&mut copied, owner);
        let mut extension = false;
        for (c, v) in &mut copied {
            if *c == 102 {
                extension = v == "{ACAD_XDICTIONARY";
            } else if extension && *c == 360 {
                *v = format!("{:X}", self.xdict(id(v)?, handle, &mut HashSet::new(), 0)?);
            }
        }
        if r.kind()=="DIMENSION" {
            let name=first(&copied,2).ok_or("Maatvoering mist geometrieblok")?.to_owned();
            let block=self.dimension_block(&name)?;
            scope_set(&mut copied,"AcDbDimension",2,block)?;
        }
        if r.kind() == "IMAGE" {
            let definition = id(first(&copied, 340).ok_or("Afbeeldingsdefinitie ontbreekt")?)?;
            let def = self
                .doc
                .by_handle(&format!("{definition:X}"))?
                .ok_or("Afbeeldingsdefinitie ontbreekt")?;
            if def.kind() != "IMAGEDEF" {
                return Err("Ongeldige gedeelde afbeeldingsdefinitie".into());
            }
            let reactor = self.allocate()?;
            scope_set(&mut copied, "AcDbRasterImage", 360, format!("{reactor:X}"))?;
            write!(self.objects,"0\nIMAGEDEF_REACTOR\n5\n{reactor:X}\n330\n{handle:X}\n100\nAcDbRasterImageDefReactor\n90\n2\n330\n{handle:X}\n").unwrap();
            self.definition_reactors
                .entry(definition)
                .or_default()
                .push(reactor);
        }
        // Underlays share their document definition, just as ezdxf's copy_data
        // does. Binding a new copy adds its entity handle to that definition's
        // reactors; omitting this leaves a one-way reference after readback.
        if matches!(r.kind(), "PDFUNDERLAY" | "DGNUNDERLAY" | "DWFUNDERLAY") {
            let definition = id(first(&copied, 340).ok_or("Underlaydefinitie ontbreekt")?)?;
            let def = self.doc.by_handle(&format!("{definition:X}"))?
                .ok_or("Underlaydefinitie ontbreekt")?;
            let expected = match r.kind() {
                "PDFUNDERLAY" => "PDFDEFINITION",
                "DGNUNDERLAY" => "DGNDEFINITION",
                _ => "DWFDEFINITION",
            };
            if def.kind() != expected || def.section() != Some("OBJECTS") {
                return Err("Ongeldige gedeelde underlaydefinitie".into());
            }
            self.definition_reactors.entry(definition).or_default().push(handle);
        }
        Ok((handle, copied))
    }
    pub fn register_block(&mut self){self.block_count+=1;}
    fn dimension_block(&mut self,name:&str)->Result<String,String>{
        let record=self.doc.records().find(|r|r.kind()=="BLOCK_RECORD"&&r.first(2).is_some_and(|v|crate::dxf_symbols::key(v)==crate::dxf_symbols::key(name))).ok_or("Maatvoering verwijst naar ontbrekend geometrieblok")?;
        let old=record.handle().ok_or("Maatvoeringblok mist handle")?;
        if self.dimension_stack.len()>=16||!self.dimension_stack.insert(old){return Err("Cyclisch of te diep maatvoeringblok".into());}
        let table=owner(record).ok_or("Maatvoeringblok mist tabel")?;
        let block_name=(1..=100_000).map(|i|format!("*D{i}")).find(|n|!self.block_names.contains(&crate::dxf_symbols::key(n))).ok_or("Geen vrije maatvoeringbloknaam")?;
        self.block_names.insert(crate::dxf_symbols::key(&block_name));
        let (new,mut t)=self.entity(record,table)?;replace(&mut t,2,block_name.clone());
        self.block_records.push_str(&encode(&t));self.register_block();
        let records=self.doc.records().filter(|r|r.section()==Some("BLOCKS")).collect::<Vec<_>>();
        let block=records.iter().copied().find(|r|r.kind()=="BLOCK"&&owner(*r)==Some(old)).ok_or("Maatvoeringblok mist BLOCK")?;
        let end=records.iter().copied().find(|r|r.kind()=="ENDBLK"&&owner(*r)==Some(old)).ok_or("Maatvoeringblok mist ENDBLK")?;
        let (_,mut t)=self.entity(block,new)?;replace(&mut t,2,block_name.clone());replace(&mut t,3,block_name.clone());let mut content=encode(&t);
        let mut map=HashMap::new();map.insert(old,new);
        for r in records {input::check(self.cancel)?;if matches!(r.kind(),"BLOCK"|"ENDBLK")||self.is_linked_column(r){continue;}let Some(target)=owner(r).and_then(|h|map.get(&h).copied())else{continue;};
            let group=self.entity_group(r,target)?;
            if let Some(old)=r.handle(){map.insert(old,id(first(&group[0],5).ok_or("Blokkopie mist handle")?)?);}
            // Preserve block coordinates and DIMENSION.insert as one placement.
            for mut t in group{if first(&t,67).is_some(){scope_set(&mut t,"AcDbEntity",67,"0".into())?;}t.retain(|(c,_)|*c!=410);content.push_str(&encode(&t));}
        }
        let (_,t)=self.entity(end,new)?;content.push_str(&encode(&t));self.blocks.push_str(&content);
        self.dimension_stack.remove(&old);Ok(block_name)
    }
    pub fn changes(&self) -> Result<Vec<Change>, String> {
        let mut changes = Vec::new();
        if self.block_count>0{
            let mut active=None;let mut table=None;let mut end=None;
            for r in self.doc.records(){if r.kind()=="TABLE"{active=r.first(2);if active==Some("BLOCK_RECORD"){table=Some(r);}}else if r.kind()=="ENDTAB"&&active==Some("BLOCK_RECORD"){end=Some(self.doc.record_start(r.index()));break;}}
            let table=table.ok_or("Bloktabel ontbreekt")?;
            if !self.block_records.is_empty(){let at=end.ok_or("Bloktabeleinde ontbreekt")?;changes.push(Change{start:at,end:at,text:self.block_records.clone()});}
            changes.push(self.doc.tag_change(table.index(),70,&(self.doc.records().filter(|r|r.kind()=="BLOCK_RECORD").count()+self.block_count).to_string())?);
            if !self.blocks.is_empty(){let at=self.doc.section_end("BLOCKS")?;changes.push(Change{start:at,end:at,text:self.blocks.clone()});}
        }
        if !self.objects.is_empty() {
            let at = self.doc.section_end("OBJECTS")?;
            changes.push(Change {
                start: at,
                end: at,
                text: self.objects.clone(),
            });
        }
        for (&handle, reactors) in &self.definition_reactors {
            let r = self
                .doc
                .by_handle(&format!("{handle:X}"))?
                .ok_or("Gedeelde raster-/underlaydefinitie ontbreekt")?;
            let mut t = tags(r);
            let mut group = false;
            let mut at = None;
            for (i, (c, v)) in t.iter().enumerate() {
                if *c == 102 && v == "{ACAD_REACTORS" {
                    group = true;
                } else if group && *c == 102 && v == "}" {
                    at = Some(i);
                    break;
                }
            }
            let values = reactors
                .iter()
                .map(|h| (330, format!("{h:X}")))
                .collect::<Vec<_>>();
            if let Some(at) = at {
                t.splice(at..at, values);
            } else {
                let at = t.iter().position(|(c, _)| *c == 100).unwrap_or(t.len());
                let mut group = vec![(102, "{ACAD_REACTORS".into())];
                group.extend(values);
                group.push((102, "}".into()));
                t.splice(at..at, group);
            }
            changes.push(self.doc.record_change(r.index(), encode(&t)));
        }
        Ok(changes)
    }
}
