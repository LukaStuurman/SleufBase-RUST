//! Deliberate conversion from a measured section axis to a native trench.
//! Full source polylines stay in the cloud; only actual section crossings enter
//! the profile. All calculations use the original double-precision coordinates.
use crate::{
    model::Vertex,
    pointcloud::{AnnotationKind, CloudCrs, PointCloudSource},
    virtual_trench::{TrenchPoint, VirtualTrench},
};

pub struct Prepared {
    pub trench: VirtualTrench,
    pub skipped: usize,
}

pub fn prepare(
    source: &PointCloudSource,
    axis_id: &str,
    selected_ids: &[String],
    label: String,
    width: f64,
) -> Result<Prepared, String> {
    source.validate()?;
    if !matches!(source.crs, CloudCrs::RdNew) {
        return Err("Bevestig eerst dat de puntenwolk RD-coördinaten (EPSG:28992) in meters bevat.".into());
    }
    if label.trim().is_empty() || label.len() > 256 {
        return Err("Geef de proefsleuf een naam van maximaal 256 tekens.".into());
    }
    let axis = source.annotations.iter().find(|a| a.id == axis_id)
        .ok_or("Kies een getekende lijn als sleufas.")?;
    if !matches!(axis.kind, AnnotationKind::Polyline) || axis.vertices.len() != 2 {
        return Err("De sleufas moet een rechte lijn met precies twee meetpunten zijn.".into());
    }
    let mut trench = VirtualTrench::new(label.trim().into(), axis.vertices[0], axis.vertices[1], width)?;
    trench.source_path = Some(source.source_path.clone());
    trench.metadata = serde_json::json!({
        "pointcloud_source_id": source.id,
        "pointcloud_axis_id": axis.id,
        "pointcloud_title": source.title,
        "pointcloud_projection": source.projection,
    });
    let mut skipped = 0;
    for annotation in source.annotations.iter().filter(|a| a.id != axis_id && selected_ids.contains(&a.id)) {
        let mut crossings = Vec::<(Vertex, Option<f64>)>::new();
        match annotation.kind {
            AnnotationKind::Point => {
                let p = annotation.vertices[0];
                let (x, y, _, overrun) = trench.project(p);
                if overrun <= 1e-6 && (p.x-x).hypot(p.y-y) <= width * 0.5 + 1e-6 {
                    crossings.push((p, None));
                }
            }
            AnnotationKind::Polyline => {
                for segment in annotation.vertices.windows(2) {
                    if let Some((p, angle)) = crossing(trench.start, trench.end, segment[0], segment[1]) {
                        if angle.abs() > 80. {
                            return Err(format!("Lijn '{}' loopt bijna evenwijdig aan de sleufas. Kies deze lijn niet voor het dwarsprofiel; de sleufbewerker ondersteunt maximaal 80 graden schuinte.", annotation.name));
                        }
                        // A vertex shared by two adjacent segments is one crossing.
                        if !crossings.iter().any(|(old, _)| (old.x-p.x).hypot(old.y-p.y) < 1e-7
                            && old.z.zip(p.z).is_none_or(|(a,b)| (a-b).abs() < 1e-7)) {
                            crossings.push((p, Some(angle)));
                        }
                    }
                }
            }
        }
        if crossings.is_empty() { skipped += 1; }
        for (position, angle) in crossings {
            let mut metadata = serde_json::json!({"pointcloud_annotation_id": annotation.id});
            if let Some(angle) = angle { metadata["crossing_angle_degrees"] = angle.into(); }
            trench.add_object(TrenchPoint {
                role: "kabel".into(), object_name: annotation.name.clone(),
                source_name: annotation.name.clone(), position,
                attributes: annotation.attributes.clone(), metadata,
            })?;
        }
    }
    trench.validate()?;
    Ok(Prepared { trench, skipped })
}

fn crossing(a: Vertex, b: Vertex, p: Vertex, q: Vertex) -> Option<(Vertex, f64)> {
    let r = [b.x-a.x,b.y-a.y];
    let s = [q.x-p.x,q.y-p.y];
    let cross = |v:[f64;2], w:[f64;2]| v[0]*w[1]-v[1]*w[0];
    let denominator = cross(r,s);
    let scale = r[0].hypot(r[1]) * s[0].hypot(s[1]);
    // Parallel or coincident lines do not define a unique cable crossing.
    if scale <= 1e-12 || denominator.abs() <= 1e-10 * scale { return None; }
    let d = [p.x-a.x,p.y-a.y];
    let t = cross(d,s)/denominator;
    let u = cross(d,r)/denominator;
    if !(-1e-8..=1.0+1e-8).contains(&t) || !(-1e-8..=1.0+1e-8).contains(&u) { return None; }
    let u = u.clamp(0.,1.);
    let position = Vertex { x:p.x+u*s[0], y:p.y+u*s[1], z:p.z.zip(q.z).map(|(a,b)|a+u*(b-a)) };
    // The native editor uses 0 degrees for a perpendicular crossing.
    let angle = (r[0]*s[0]+r[1]*s[1]).atan2(denominator).to_degrees();
    // Direction-independent: reversing either line preserves the same cable.
    let angle = (angle + 90.).rem_euclid(180.) - 90.;
    Some((position, angle))
}
