//! Second-pass readability optimiser for native profile labels.
//!
//! The legacy solver remains the source of valid, deterministic placements.
//! This pass only moves an annotation when actual leader intersections,
//! label/leader contact, or label overlap is detected. It never moves the
//! measured cable/pipe anchor, marker, or band geometry.
//!
//! Avoiding AABB-only leader tests is essential: diagonal leader bounding boxes
//! can overlap without crossing, while a thin diagonal can pass through a
//! neighbouring text block even if the two text boxes do not overlap.
use super::{Entry, Options, Placed, Box4};
use crate::{input, profile_surface};
use std::sync::atomic::AtomicBool;

const MAX_CROWDED: usize = 240;
const PENALTY: f64 = 1_000_000.;
const EPS: f64 = 1e-9;
type Point = [f64; 2];
type Segment = [Point; 2];

#[derive(Clone, Copy)]
struct Shape {
    text: Box4,
    route: Segment,
}

fn text_shape(entry: &Entry, placement: &Placed, width: f64) -> Shape {
    let scale = placement.marker_scale;
    let gap = 0.05f64.max(scale * 2.5);
    let px = 0.027f64.max(scale * 0.6);
    let py = 0.01f64.max(scale * 0.35);
    let x = placement.text_insert[0] - 0.5 * scale;
    let y = placement.text_insert[1];
    Shape {
        text: [x - px, y - py, x + gap + px, y + width + py],
        route: [[entry.anchor_x, entry.leader_block_start_y], placement.text_insert],
    }
}
fn area(a: Box4, b: Box4) -> f64 {
    let width = a[2].min(b[2]) - a[0].max(b[0]);
    let height = a[3].min(b[3]) - a[1].max(b[1]);
    width.max(0.) * height.max(0.)
}
fn expand(b: Box4, m: f64) -> Box4 {
    // A hard/soft obstacle is often a segment, so its endpoint order is not
    // guaranteed to match the order of the corners of a rectangle.
    [b[0].min(b[2])-m, b[1].min(b[3])-m,
        b[0].max(b[2])+m, b[1].max(b[3])+m]
}
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
}
fn near(a: Point, b: Point) -> bool {
    (a[0]-b[0]).abs()<EPS && (a[1]-b[1]).abs()<EPS
}
fn between(a: f64, b: f64, x: f64) -> bool {
    x >= a.min(b)-EPS && x <= a.max(b)+EPS
}
fn on_segment(a: Point, b: Point, c: Point) -> bool {
    cross(a,b,c).abs()<EPS && between(a[0],b[0],c[0])
        && between(a[1],b[1],c[1])
}
fn intersect(a: Segment, b: Segment) -> bool {
    let [p,q]=a;
    let [r,s]=b;
    if near(p,r) || near(p,s) || near(q,r) || near(q,s) {
        // A shared cable origin or the same annotation attachment is a
        // legitimate junction, not an accidental crossing of two leaders.
        return false;
    }
    let c1=cross(p,q,r);
    let c2=cross(p,q,s);
    let c3=cross(r,s,p);
    let c4=cross(r,s,q);
    if (c1>EPS && c2 < -EPS || c1 < -EPS && c2 > EPS)
        && (c3>EPS && c4 < -EPS || c3 < -EPS && c4>EPS) {
        return true;
    }
    (c1.abs()<EPS && on_segment(p,q,r))
        || (c2.abs()<EPS && on_segment(p,q,s))
        || (c3.abs()<EPS && on_segment(r,s,p))
        || (c4.abs()<EPS && on_segment(r,s,q))
}
fn in_box(p: Point, b: Box4) -> bool {
    p[0]>=b[0]-EPS && p[0]<=b[2]+EPS
        && p[1]>=b[1]-EPS && p[1]<=b[3]+EPS
}
fn hits_box(line: Segment, rect: Box4) -> bool {
    if in_box(line[0], rect) || in_box(line[1], rect) {return true;}
    let [left,bottom,right,top]=rect;
    let corners=[[left,bottom],[right,bottom],[right,top],[left,top]];
    corners.into_iter().enumerate().any(|(i,a)|intersect(line,[a,corners[(i+1)%4]]))
}
#[derive(Clone, Copy)]
struct Quality { bad: u32, total: f64 }
impl Quality {
    fn add(&mut self, collisions: f64) {
        if collisions>0. {
            self.bad=self.bad.saturating_add(1);
            self.total+=PENALTY+collisions.min(10.)*1000.;
        }
    }
    fn better(self, other: Self) -> bool {
        self.bad < other.bad || self.bad == other.bad && self.total + 1e-8 < other.total
    }
}
fn quality(
    i:usize, placement:&Placed, shape:Shape, entries:&[Entry],
    shapes:&[Shape], options:&Options<'_>, margin:f64,
) -> Quality {
    let e=&entries[i];
    let mut score=Quality{bad:0,total:0.};
    let dx=(placement.text_insert[0]-e.anchor_x).abs();
    let dy=(placement.text_insert[1]-e.leader_top_y).abs();
    let leader_length=((placement.text_insert[0]-e.anchor_x).powi(2)
        +(placement.text_insert[1]-e.leader_block_start_y).powi(2)).sqrt();
    // An easy-to-read arrangement remains tied to each measured cable.
    // A short, close leader always wins among equally collision-free layouts.
    score.total=dx*150.+dy*180.+leader_length*13.;
    if let Some(left)=options.min_x {
        score.add((left-shape.text[0]).max(0.)*100.);
    }
    if let Some(right)=options.max_x {
        score.add((shape.text[2]-right).max(0.)*100.);
    }
    for &b in options.text {
        score.add(area(shape.text,expand(b,margin)));
        if hits_box(shape.route,expand(b,margin*0.3)){score.add(1.);}
    }
    for &b in options.hard {
        score.add(area(shape.text,expand(b,margin)));
        // Hard/soft profile geometry is stored as line endpoints, not text
        // rectangles. Compare the real segments for crossings; their large
        // diagonal bounding rectangles would create false positives.
        if intersect(shape.route,[[b[0],b[1]],[b[2],b[3]]]) {score.add(1.);}
    }
    for &b in options.soft {
        score.total+=area(shape.text,expand(b,margin))*8000.;
        if intersect(shape.route,[[b[0],b[1]],[b[2],b[3]]]) {
            score.total+=1500.;
        }
    }
    let left=shape.text[0].min(shape.route[0][0]).min(shape.route[1][0])-margin;
    let right=shape.text[2].max(shape.route[0][0]).max(shape.route[1][0])+margin;
    for (j,other) in shapes.iter().enumerate() {
        if i==j {continue;}
        // A large profile can contain hundreds of widely separated objects.
        // Their leaders, labels and fixed connectors cannot intersect if
        // these full horizontal envelopes are disjoint.
        let other_left=other.text[0].min(other.route[0][0]).min(other.route[1][0]);
        let other_right=other.text[2].max(other.route[0][0]).max(other.route[1][0]);
        if right<other_left || left>other_right {continue;}
        score.add(area(shape.text,expand(other.text,margin)));
        if intersect(shape.route,other.route) { score.add(2.); }
        // Catch leaders that cut directly through somebody else's wording,
        // including the opposite case where a moved label covers a leader.
        if hits_box(shape.route,expand(other.text,margin*0.35)){score.add(1.);}
        if hits_box(other.route,expand(shape.text,margin*0.35)){score.add(1.);}
        // Preserve existing, measured band/axis connection lines.
        let linked=&entries[j];
        let band_x=linked.band_x.unwrap_or(linked.anchor_x);
        let y=if (band_x-linked.anchor_x).abs()>1e-6 {
            linked.band_connector_y
        } else {linked.band_y};
        let axis=[[linked.anchor_x,linked.leader_line_start_y],[linked.anchor_x,y]];
        if intersect(shape.route,axis) {score.add(1.);}
    }
    score
}
fn widths(entries:&[Entry])->Vec<f64>{
    entries.iter().map(|entry|
        profile_surface::width(&entry.description,0.03)
            .max(profile_surface::width(&entry.depth_text,0.03))
    ).collect()
}
fn candidate(entry:&Entry,scale:f64,ix:i32,iy:i32) -> Placed {
    let xstep=0.05f64.max(scale*2.5);
    let ystep=0.085f64.max(scale*2.);
    Placed {
        entry_index:0,
        text_insert:[entry.anchor_x+f64::from(ix)*xstep,
            entry.leader_top_y+f64::from(iy)*ystep],
        marker_scale:scale,
    }
}
/// Improve a valid legacy solution without changing its DXF structure.
/// A bounded, deterministic three-pass local search uses the actual diagonal
/// leader segments, not just their bounding rectangles. If there is no room
/// for all annotations the minimum-collision placement is retained; no text
/// or source cable is silently dropped.
pub(super) fn improve(
    entries:&[Entry], options:&Options<'_>, baseline:Vec<Placed>,
    cancel:&AtomicBool,
)->Result<Vec<Placed>,String>{
    if !options.avoid_collisions || entries.len()<2 || entries.len()>MAX_CROWDED {
        return Ok(baseline);
    }
    input::check(cancel)?;
    let count=entries.len();
    let w=widths(entries);
    let mut positions=vec![0usize;count];
    for (slot,p) in baseline.iter().enumerate(){
        if p.entry_index>=count {return Err("Onjuiste verwijzingsindex".into());}
        positions[p.entry_index]=slot;
    }
    let mut shapes=(0..count).map(|i|{
        let p=&baseline[positions[i]];
        text_shape(&entries[i],p,w[i])
    }).collect::<Vec<_>>();
    let mut result=baseline;
    let mut order=(0..count).collect::<Vec<_>>();
    order.sort_by(|&a,&b|{
        entries[a].anchor_x.total_cmp(&entries[b].anchor_x)
            .then_with(||entries[a].leader_top_y.total_cmp(&entries[b].leader_top_y))
            .then_with(||a.cmp(&b))
    });
    // A larger group can use more nearby lanes, but bounded search prevents
    // huge profiles from blocking the application for minutes.
    let steps: i32=if count<=20 {11} else if count<=80 {17} else {22};
    let levels: i32=if count<=20 {10} else if count<=80 {15} else {19};
    let margin=0.018;
    for pass in 0..3 {
        let mut improved=0;
        if pass>0 {order.reverse();}
        for &i in &order {
            input::check(cancel)?;
            let e=&entries[i];
            let current=result[positions[i]].clone();
            let mut best=current.clone();
            let mut best_score=quality(i,&best,shapes[i],entries,&shapes,options,margin);
            if best_score.bad==0 {continue;}
            // Try the shortest labels first, then increasingly displaced
            // positions to keep annotations close to their own pipe/cable.
            for radius in 0..=steps.max(levels) {
                if radius%3==0 {input::check(cancel)?;}
                for xi in -steps..=steps {
                    for yi in -levels..=levels {
                        if xi.abs().max(yi.abs())!=radius {continue;}
                        let mut p=candidate(e,current.marker_scale,xi,yi);
                        p.entry_index=i;
                        if p.text_insert[1] <= e.leader_block_start_y + 0.8*current.marker_scale {
                            continue;
                        }
                        let shape=text_shape(e,&p,w[i]);
                        let score=quality(i,&p,shape,entries,&shapes,options,margin);
                        if score.better(best_score) {
                            best_score=score;
                            best=p;
                        }
                    }
                }
                // Once clean, don't search farther lanes merely to change
                // the appearance; still try the immediately adjacent radius.
                if best_score.bad==0 && radius>=2 {break;}
            }
            if best.text_insert!=current.text_insert {
                shapes[i]=text_shape(e,&best,w[i]);
                result[positions[i]]=best;
                improved+=1;
            }
        }
        if improved==0 {break;}
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_real_crossings_without_false_diagonal_bbox_positives(){
        assert!(intersect([[0.,0.],[2.,2.]],[[0.,2.],[2.,0.]]));
        assert!(!intersect([[0.,0.],[1.,1.]],[[0.,2.],[1.,3.]]));
        assert!(!intersect([[0.,0.],[2.,2.]],[[0.,0.],[2.,0.]]));
        assert!(hits_box([[0.,0.],[3.,3.]],[1.,1.,2.,2.]));
        assert!(!hits_box([[0.,0.],[1.,0.]],[2.,2.,3.,3.]));
    }
    #[test]
    fn crowded_labels_retain_close_measured_anchors() {
        let cancel=AtomicBool::new(false);
        let make=|x:f64|Entry {
            anchor_x:x,leader_line_start_y:0.,leader_block_start_y:0.02,
            leader_top_y:0.3,band_y:-0.5,band_connector_y:-0.45,
            band_x:Some(x),leader_scale:Some(0.02),leader_color:7,
            description:"150 Al kabel".into(),depth_text:"Diepte: 0,70".into(),
        };
        let entries=(0..12).map(|i|make(2.0+i as f64*0.035)).collect::<Vec<_>>();
        let base=entries.iter().enumerate().map(|(i,e)|Placed{
            entry_index:i,text_insert:[e.anchor_x,e.leader_top_y],marker_scale:0.02
        }).collect::<Vec<_>>();
        let options=Options{min_x:Some(1.0),max_x:Some(5.0),..Default::default()};
        let improved=improve(&entries,&options,base,&cancel).unwrap();
        let widths=widths(&entries);
        let shapes=improved.iter().map(|p|text_shape(&entries[p.entry_index],p,widths[p.entry_index]))
            .collect::<Vec<_>>();
        let cross=|shapes:&[Shape]|{
            shapes.iter().enumerate().flat_map(|(i,a)|
                shapes.iter().skip(i+1).map(move|b|
                    u32::from(intersect(a.route,b.route) || area(a.text,b.text)>0.)
                )).sum::<u32>()
        };
        let before=entries.iter().enumerate().map(|(i,e)|text_shape(e,&Placed{
            entry_index:i,text_insert:[e.anchor_x,e.leader_top_y],marker_scale:0.02
        },widths[i])).collect::<Vec<_>>();
        assert!(cross(&shapes)<cross(&before));
        assert!(improved.iter().all(|p|p.text_insert[0].is_finite() && p.text_insert[1].is_finite()));
    }
}
