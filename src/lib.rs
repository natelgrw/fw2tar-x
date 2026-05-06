pub mod analysis;
pub mod archive;
pub mod args;
mod error;
pub mod extractors;
pub mod metadata;
pub mod scanner;

use analysis::{extract_and_process, ExtractionResult};
pub use error::Fw2tarError;
use metadata::{ArchiveMetadata, FirmwareMetadata, Metadata};

use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, fs, thread};

pub enum BestExtractor {
    Best(&'static str),
    Only(&'static str),
    Identical(&'static str),
    None,
}

pub fn main(args: args::Args) -> Result<(BestExtractor, PathBuf), Fw2tarError> {
    if !args.firmware.is_file() {
        if args.firmware.exists() {
            return Err(Fw2tarError::FirmwareNotAFile(args.firmware));
        } else {
            return Err(Fw2tarError::FirmwareDoesNotExist(args.firmware));
        }
    }

    let output = args
        .output
        .unwrap_or_else(|| {
            // Use file_stem() which should behave like Python's Path.stem
            if let Some(stem) = args.firmware.file_stem() {
                args.firmware.with_file_name(stem)
            } else {
                // No stem available, use as-is
                args.firmware.clone()
            }
        });

    let selected_output_path = {
        // Simple string append to avoid with_extension() being greedy
        let file_name = output.file_name().unwrap().to_string_lossy();
        output.with_file_name(format!("{}.rootfs.tar.gz", file_name))
    };

    if selected_output_path.exists() && !args.force {
        return Err(Fw2tarError::OutputExists(selected_output_path));
    }

    let metadata = Metadata {
        input_hash: analysis::sha1_file(&args.firmware).unwrap_or_default(),
        file: args.firmware.display().to_string(),
        fw2tar_command: env::args().collect(),
    };

    let detected_regions = scanner::scan_firmware(&args.firmware).unwrap_or_else(|e| {
        log::error!("Failed to run native byte scanner: {}", e);
        Vec::new()
    });
    log::info!("Native byte scanner detected {} potential regions", detected_regions.len());

    extractors::set_timeout(args.timeout);

    let extractors: Vec<_> = args
        .extractors
        .map(|extractors| {
            extractors
                .split(",")
                .map(String::from)
                .filter(|e| e.to_lowercase() != "none")
                .collect()
        })
        .unwrap_or_else(|| {
            extractors::all_extractor_names()
                .map(String::from)
                .collect()
        });

    let results: Mutex<Vec<ExtractionResult>> = Mutex::new(Vec::new());

    let removed_devices: Option<Mutex<HashSet<PathBuf>>> =
        args.log_devices.then(|| Mutex::new(HashSet::new()));

    thread::scope(|threads| -> Result<(), Fw2tarError> {
        for extractor_name in extractors {
            let extractor = extractors::get_extractor(&extractor_name)
                .ok_or_else(|| Fw2tarError::InvalidExtractor(extractor_name.clone()))?;

            threads.spawn(|| {
                if let Err(e) = extract_and_process(
                    extractor,
                    &args.firmware,
                    &output,
                    args.scratch_dir.as_deref(),
                    args.loud,
                    args.primary_limit,
                    args.secondary_limit,
                    &results,
                    &metadata,
                    removed_devices.as_ref(),
                ) {
                    log::info!("{} error: {e}", extractor.name());
                }
            });
        }

        Ok(())
    })?;

    if let Some(removed_devices) = removed_devices {
        let mut removed_devices = removed_devices
            .into_inner()
            .unwrap()
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        removed_devices.sort();

        if removed_devices.is_empty() {
            log::warn!("No device files were found during extraction, skipping writing log");
        } else {
            let devices_log_path = {
                // Simple string append to avoid with_extension() being greedy
                let file_name = output.file_name().unwrap().to_string_lossy();
                output.with_file_name(format!("{}.devices.log", file_name))
            };
            fs::write(
                devices_log_path,
                removed_devices.join("\n"),
            )
            .unwrap();
        }
    }

    let results = results.lock().unwrap();
    let mut best_results: Vec<_> = results.iter().filter(|&res| res.index == 0).collect();

    let result = if best_results.is_empty() {
        Ok((BestExtractor::None, selected_output_path.clone()))
    } else if best_results.len() == 1 {
        Ok((BestExtractor::Only(best_results[0].extractor), selected_output_path.clone()))
    } else {
        best_results.sort_by_key(|res| Reverse((res.file_node_count, res.extractor == "unblob")));

        Ok((BestExtractor::Best(best_results[0].extractor), selected_output_path.clone()))
    };

    if !best_results.is_empty() {
        let best_result = best_results[0];
        fs::rename(&best_result.path, &selected_output_path).unwrap();
    }

    let image_size = fs::metadata(&args.firmware).map(|m| m.len()).unwrap_or(0);

    let archives: Vec<ArchiveMetadata> = results
        .iter()
        .map(|res| {
            let final_path = if !best_results.is_empty() && res.path == best_results[0].path {
                &selected_output_path
            } else {
                &res.path
            };

            ArchiveMetadata {
                path: final_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                extractor: res.extractor.to_string(),
                rootfs_score: res.rootfs_score,
                was_merged: false,
                file_node_count: res.file_node_count,
                archive_hash: res.archive_hash.clone(),
            }
        })
        .collect();

    let fw_metadata = FirmwareMetadata {
        input_hash: analysis::sha1_file(&args.firmware).unwrap_or_default(),
        file: args.firmware.display().to_string(),
        image_size,
        fw2tar_command: env::args().collect(),
        detected_regions,
        archives,
    };

    let fw_meta_path = {
        let file_name = output.file_name().unwrap().to_string_lossy();
        output.with_file_name(format!("{}.fw2tar-meta.json", file_name))
    };

    fs::write(fw_meta_path, serde_json::to_string_pretty(&fw_metadata).unwrap()).unwrap();

    result
}
