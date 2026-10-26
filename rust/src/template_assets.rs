//! Prepare independent assets in bounded light/heavy phases. DXF mutation and
//! publication remain on the export coordinator after every worker has joined.
use super::Request;
use crate::{
    asset_budget::Budget, export_control::Control, map_export, resource_policy::ResourcePolicy,
    template_photo, template_raster, virtual_raster, Bounds,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const BUNDLE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const FILE_LIMIT: u64 = 256 * 1024 * 1024;

pub(super) enum Kind<'a> {
    Copy(&'a Path),
    Tiff {
        source: &'a template_raster::Source,
        axis: Option<[[f64; 2]; 2]>,
        reverse: bool,
    },
    Virtual {
        source: &'a virtual_raster::Source,
        reverse: bool,
    },
    Map {
        source: &'a map_export::Source,
        bounds: Bounds,
        fallback: Option<map_export::Raster<'a>>,
        reference: Option<[f64; 2]>,
    },
    Photo(&'a template_photo::Photo),
}
impl Kind<'_> {
    fn heavy(&self) -> bool {
        match self {
            Self::Map {
                source, fallback, ..
            } => {
                source.settings.dpi > 300
                    || matches!(source.background, map_export::Background::Png { .. })
                    || (source.include_tiff
                        && (source.tiff.is_some()
                            || matches!(fallback, Some(map_export::Raster::Tiff(_)))
                            || matches!(fallback, Some(map_export::Raster::Virtual(s)) if s.quality > 1.)))
            }
            // A prepared PNG is decoded for validation; count its allocation in
            // the heavy lane, even though copying the compressed bytes is cheap.
            _ => true,
        }
    }
}
struct Job<'a> {
    kind: Kind<'a>,
    destination: PathBuf,
    aliases: Vec<PathBuf>,
}
#[derive(Default)]
pub(super) struct Batch<'a> {
    jobs: Vec<Job<'a>>,
}
impl<'a> Batch<'a> {
    pub(super) fn push(&mut self, kind: Kind<'a>, destination: PathBuf) -> usize {
        let index = self.jobs.len();
        self.jobs.push(Job {
            kind,
            destination,
            aliases: Vec::new(),
        });
        index
    }
    pub(super) fn alias(&mut self, index: usize, destination: PathBuf) {
        self.jobs[index].aliases.push(destination);
    }
    pub(super) fn run(
        &self,
        request: &Request,
        control: &Control,
        initial_total: u64,
        policy: &ResourcePolicy,
        progress: &mut impl FnMut(&str),
    ) -> Result<u64, String> {
        let remaining = BUNDLE_LIMIT
            .checked_sub(initial_total)
            .ok_or("Sjabloonafbeeldingen overschrijden het exportbudget")?;
        let budget = Budget::new(remaining);
        // Never cancel the caller's control on a local asset error: a failed
        // export can be retried. The scheduler propagates parent cancellation
        // to this batch flag before joining all in-flight workers.
        let batch = Control::default();
        let mut completed = 0;
        for heavy in [false, true] {
            let indices = self
                .jobs
                .iter()
                .enumerate()
                .filter_map(|(index, job)| (job.kind.heavy() == heavy).then_some(index))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                continue;
            }
            // Explicit environment overrides cannot start unbounded threads or
            // full TIFF decoders. Defaults from the resource policy fit here.
            let workers = if heavy {
                policy.heavy_raster_workers.clamp(1, 4)
            } else {
                policy.light_map_workers.clamp(1, 32)
            }
            .min(indices.len());
            progress(if heavy {
                "Rasters en foto's voorbereiden"
            } else {
                "Kaartbladen voorbereiden"
            });
            control.check()?;
            let renderers = (0..workers)
                .map(|_| Mutex::new(map_export::Renderer::default()))
                .collect::<Vec<_>>();
            crate::template_fetch::ordered(
                indices.len(),
                workers,
                control,
                |worker, index, _| {
                    let job = &self.jobs[indices[index]];
                    batch.check()?;
                    match &job.kind {
                        Kind::Copy(source) => {
                            copy_asset(source, &job.destination, &batch, &budget)?;
                        }
                        Kind::Tiff {
                            source,
                            axis,
                            reverse,
                        } => {
                            let mut oriented = (*source).clone();
                            oriented.reverse_orientation = *reverse;
                            let raster = template_raster::from_tiff(
                                &oriented,
                                &request.roads,
                                &request.terrain_boundaries,
                                *axis,
                                batch.flag(),
                            )?;
                            template_raster::save_image_budgeted(
                                &raster.image,
                                &job.destination,
                                batch.flag(),
                                &budget,
                            )?;
                        }
                        Kind::Virtual { source, reverse } => {
                            let raster = virtual_raster::prepare_for_template_direction(
                                source,
                                *reverse,
                                batch.flag(),
                            )?;
                            template_raster::save_image_budgeted(
                                &raster.image,
                                &job.destination,
                                batch.flag(),
                                &budget,
                            )?;
                        }
                        Kind::Map {
                            source,
                            bounds,
                            fallback,
                            reference,
                        } => {
                            let fallback = match fallback {
                                Some(map_export::Raster::Tiff(s)) => {
                                    Some(map_export::Raster::Tiff(s))
                                }
                                Some(map_export::Raster::Virtual(s)) => {
                                    Some(map_export::Raster::Virtual(s))
                                }
                                None => None,
                            };
                            let page = renderers[worker]
                                .lock()
                                .map_err(|_| "Kaartwerker is onverwacht gestopt")?
                                .render_with_raster(
                                    source,
                                    *bounds,
                                    fallback,
                                    *reference,
                                    batch.flag(),
                                )?;
                            template_raster::save_image_budgeted(
                                &page.image,
                                &job.destination,
                                batch.flag(),
                                &budget,
                            )?;
                        }
                        Kind::Photo(photo) => {
                            template_photo::prepare_budgeted(
                                photo,
                                &job.destination,
                                &batch,
                                &budget,
                            )?;
                        }
                    }
                    for alias in &job.aliases {
                        copy_asset(&job.destination, alias, &batch, &budget)?;
                    }
                    batch.check()
                },
                |_| {
                    completed += 1;
                    progress(&format!(
                        "Afbeelding {} van {} gereed",
                        completed,
                        self.jobs.len()
                    ));
                    Ok(())
                },
                || {
                    batch.cancel();
                },
            )?;
        }
        control.check()?;
        Ok(initial_total + budget.used())
    }
}

fn copy_asset(
    source: &Path,
    destination: &Path,
    control: &Control,
    budget: &Budget,
) -> Result<u64, String> {
    control.check()?;
    let mut source =
        File::open(source).map_err(|e| format!("Voorbereide afbeelding ontbreekt: {e}"))?;
    let size = source.metadata().map_err(|e| e.to_string())?.len();
    if size > FILE_LIMIT {
        return Err("Voorbereide afbeelding is te groot".into());
    }
    if size > budget.remaining() {
        return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        control.check()?;
        let count = match source.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            r => r.map_err(|e| e.to_string())?,
        };
        if count == 0 {
            break;
        }
        copied += count as u64;
        if copied > FILE_LIMIT {
            return Err("Voorbereide afbeelding is te groot".into());
        }
        let mut rest = &buffer[..count];
        while !rest.is_empty() {
            control.check()?;
            match budget.write(&mut output, rest) {
                Ok(0) => return Err("Afbeelding kon niet volledig geschreven worden".into()),
                Ok(n) => rest = &rest[n..],
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    output.flush().map_err(|e| e.to_string())?;
    drop(output);
    control.check()?;
    if copied == 0 {
        return Err("Voorbereide afbeelding is leeg".into());
    }
    let mut reader = image::ImageReader::open(destination).map_err(|e| e.to_string())?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Ongeldige voorbereide PNG: {e}"))?;
    control.check()?;
    Ok(copied)
}

#[cfg(test)]
#[path = "template_assets_tests.rs"]
mod tests;
