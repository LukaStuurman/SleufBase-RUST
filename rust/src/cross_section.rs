//! Local cable/pipe marker spans, matching marxact_local_cross_section_patch.py.
//! A concave boundary can contain multiple spans; measured lateral offset selects
//! the correct one. No global-width fallback may draw across a polygon gap.
use crate::{model::Vertex, virtual_trench::VirtualTrench};

pub type XY = (f64, f64);
const EPSILON: f64 = 1e-9;
const TOLERANCE: f64 = 1e-8;
fn cross(a: XY, b: XY) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn inside(p: XY, polygon: &[XY]) -> bool {
    let mut result = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + polygon.len() - 1) % polygon.len()];
        if (a.1 > p.1) != (b.1 > p.1) {
            let x = (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0;
            if p.0 < x {
                result = !result;
            }
        }
    }
    result
}

pub fn local_segment(polygon: &[XY], origin: XY, normal: XY, preferred: f64) -> Option<(XY, XY)> {
    if polygon.len() < 3
        || polygon.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
        || ![origin.0, origin.1, normal.0, normal.1, preferred]
            .iter()
            .all(|n| n.is_finite())
    {
        return None;
    }
    let length = normal.0.hypot(normal.1);
    if length <= EPSILON || !length.is_finite() {
        return None;
    }
    let direction = (normal.0 / length, normal.1 / length);
    let mut hits = Vec::new();
    let point = |t: f64| (origin.0 + direction.0 * t, origin.1 + direction.1 * t);
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let edge = (b.0 - a.0, b.1 - a.1);
        let relative = (a.0 - origin.0, a.1 - origin.1);
        let denominator = cross(direction, edge);
        if denominator.abs() <= EPSILON {
            if cross(relative, direction).abs() <= TOLERANCE {
                hits.push(relative.0 * direction.0 + relative.1 * direction.1);
                hits.push((b.0 - origin.0) * direction.0 + (b.1 - origin.1) * direction.1);
            }
            continue;
        }
        let t = cross(relative, edge) / denominator;
        let segment = cross(relative, direction) / denominator;
        if (-TOLERANCE..=1.0 + TOLERANCE).contains(&segment) {
            hits.push(t);
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() <= TOLERANCE);
    let intervals: Vec<_> = hits
        .windows(2)
        .filter(|v| v[1] - v[0] > TOLERANCE && inside(point((v[0] + v[1]) * 0.5), polygon))
        .map(|v| (v[0], v[1]))
        .collect();
    let containing = intervals
        .iter()
        .filter(|v| v.0 - TOLERANCE <= preferred && preferred <= v.1 + TOLERANCE)
        .min_by(|a, b| {
            ((a.0 + a.1) * 0.5 - preferred)
                .abs()
                .total_cmp(&((b.0 + b.1) * 0.5 - preferred).abs())
        });
    let best = containing.or_else(|| {
        intervals.iter().min_by(|a, b| {
            (preferred - a.0)
                .abs()
                .min((preferred - a.1).abs())
                .total_cmp(&(preferred - b.0).abs().min((preferred - b.1).abs()))
        })
    })?;
    Some((point(best.0), point(best.1)))
}

#[derive(Clone, Debug)]
pub struct Marker {
    pub start: Vertex,
    pub end: Vertex,
    pub object_index: usize,
    pub chainage: f64,
}

/// Direction of a cable relative to the trench: zero is perpendicular.
pub fn crossing_direction(normal:XY,metadata:&serde_json::Value)->XY {
    let angle=metadata.get("crossing_angle_degrees").and_then(serde_json::Value::as_f64)
        .filter(|a|a.is_finite()&&a.abs()<=80.).unwrap_or(0.);
    if angle==0.{return normal;}
    let angle=angle.to_radians();
    (normal.0*angle.cos()+normal.1*angle.sin(),normal.1*angle.cos()-normal.0*angle.sin())
}
/// Profile intersection of an oblique measured cable with the trench axis.
/// Unannotated legacy points retain their exact original coordinates.
pub fn object_axis_position(trench:&VirtualTrench,object:&crate::virtual_trench::TrenchPoint)->Vertex{
    let angle=object.metadata.get("crossing_angle_degrees").and_then(serde_json::Value::as_f64).unwrap_or(0.);
    if angle==0.||!angle.is_finite()||angle.abs()>80.{return object.position;}
    let length=trench.length();if length<=EPSILON{return object.position;}
    let normal=(-(trench.end.y-trench.start.y)/length,(trench.end.x-trench.start.x)/length);
    let d=crossing_direction(normal,&object.metadata);
    let offset=(object.position.x-trench.start.x)*normal.0+(object.position.y-trench.start.y)*normal.1;
    let distance=-offset/(d.0*normal.0+d.1*normal.1);
    Vertex{x:object.position.x+distance*d.0,y:object.position.y+distance*d.1,z:object.position.z}
}
/// Preserve the original perpendicular-span rule when no angle was supplied.
pub fn object_segment(trench:&VirtualTrench,object:&crate::virtual_trench::TrenchPoint)->Option<(XY,XY)>{
    object_segment_on_boundary(trench,object,&trench.measured_boundary())
}
pub fn object_segment_on_boundary(trench:&VirtualTrench,object:&crate::virtual_trench::TrenchPoint,polygon:&[XY])->Option<(XY,XY)>{
    let length=trench.length();if length<=EPSILON{return None;}
    let normal=(-(trench.end.y-trench.start.y)/length,(trench.end.x-trench.start.x)/length);
    let direction=crossing_direction(normal,&object.metadata);
    let(x,y,_,_)=trench.project(object.position);
    let angled=object.metadata.get("crossing_angle_degrees").and_then(serde_json::Value::as_f64).is_some_and(|v|v!=0.&&v.is_finite()&&v.abs()<=80.);
    let(origin,preferred)=if angled{((object.position.x,object.position.y),0.)}else{((x,y),(object.position.x-x)*normal.0+(object.position.y-y)*normal.1)};
    local_segment(polygon,origin,direction,preferred)
}

/// Prepare once after geometry edits; callers can reuse these spans during pan/zoom.
pub fn markers(trench: &VirtualTrench) -> Vec<Marker> {
    markers_with_cancel(trench, &crate::input::NEVER_CANCEL).unwrap_or_default()
}
pub fn markers_with_cancel(
    trench: &VirtualTrench,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<Marker>, String> {
    crate::input::check(cancel)?;
    let length = trench.length();
    if length <= EPSILON || !length.is_finite() {
        return Ok(Vec::new());
    }
    let polygon=trench.measured_boundary();
    let mut output = Vec::new();
    for (index, object) in trench.objects.iter().enumerate() {
        crate::input::check(cancel)?;
        let p = object.position;
        let (_, _, chainage, _) = trench.project(object_axis_position(trench,object));
        let Some((start, end)) = object_segment_on_boundary(trench,object,&polygon) else {
            continue;
        };
        output.push(Marker {
            start: Vertex {
                x: start.0,
                y: start.1,
                z: p.z,
            },
            end: Vertex {
                x: end.0,
                y: end.1,
                z: p.z,
            },
            object_index: index,
            chainage,
        })
    }
    Ok(output)
}

#[cfg(test)]
mod crossing_tests {
    use super::*;
    #[test]
    fn oblique_cables_cross_the_selected_point_and_stay_inside_the_boundary(){
        let mut t=VirtualTrench::new("PS".into(),Vertex{x:0.,y:0.,z:None},Vertex{x:10.,y:0.,z:None},0.6).unwrap();
        let mut p=crate::virtual_trench::TrenchPoint{metadata:serde_json::Value::Null,role:"kabel".into(),object_name:"LS".into(),source_name:"LS".into(),position:Vertex{x:5.,y:0.,z:Some(1.)},attributes:Default::default()};
        let perpendicular=object_segment(&t,&p).unwrap();
        assert_eq!(perpendicular.0.0,5.);assert_eq!(perpendicular.1.0,5.);
        p.metadata=serde_json::json!({"crossing_angle_degrees":45.});
        let(a,b)=object_segment(&t,&p).unwrap();
        assert!((a.0-4.7).abs()<1e-9&&(b.0-5.3).abs()<1e-9);
        assert!((a.1+0.3).abs()<1e-9&&(b.1-0.3).abs()<1e-9);
        std::mem::swap(&mut t.start,&mut t.end);
        let(c,d)=object_segment(&t,&p).unwrap();
        assert!((a.0-d.0).abs()<1e-9&&(b.0-c.0).abs()<1e-9);
        p.metadata=serde_json::json!({"crossing_angle_degrees":0.});std::mem::swap(&mut t.start,&mut t.end);
        assert_eq!(object_segment(&t,&p).unwrap(),perpendicular);
    }
}
#[cfg(test)]
mod crossing_profile_tests {
    use super::*;
    #[test]
    fn an_offset_oblique_measurement_projects_to_its_true_profile_intersection(){
        let mut t=VirtualTrench::new("PS".into(),Vertex{x:0.,y:0.,z:None},Vertex{x:10.,y:0.,z:None},0.6).unwrap();
        t.objects.push(crate::virtual_trench::TrenchPoint{metadata:serde_json::json!({"crossing_angle_degrees":45.}),role:"leiding".into(),object_name:"Waterleiding".into(),source_name:"water".into(),position:Vertex{x:5.,y:0.1,z:Some(-1.)},attributes:Default::default()});
        let point=object_axis_position(&t,&t.objects[0]);assert!((point.x-4.9).abs()<1e-9&&point.y.abs()<1e-9);assert_eq!(point.z,Some(-1.));
        let data=t.to_dataset(1,false);let crate::Geometry::Point(actual)=data.features[0].geometry else{panic!("point expected")};assert_eq!(actual,point);
        assert!((markers(&t)[0].chainage-4.9).abs()<1e-9);
        assert_eq!(t.objects[0].position.x,5.);assert_eq!(t.objects[0].position.y,0.1);
    }
}
