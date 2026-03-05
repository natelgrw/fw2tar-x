use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;
use std::{env, io};

use sha1::{Digest, Sha1};
use tempfile::TempDir;
use thiserror::Error;

pub mod directory_executables;
pub mod find_linux_filesystems;

use crate::archive::tar_fs;
use crate::extractors::{ExtractError, Extractor};
use crate::metadata::Metadata;
use find_linux_filesystems::find_linux_filesystems;

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub extractor: &'static str,
    pub index: usize,
    pub size: u64,
    pub num_files: usize,
    pub primary: bool,
    pub archive_hash: String,
    pub file_node_count: usize,
    pub path: PathBuf,
    pub rootfs_score: f64,
}

#[derive(Error, Debug)]
pub enum ExtractProcessError {
    #[error("Failed to create temporary directory ({0:?})")]
    TempDirFail(io::Error),

    #[error("Failed to extract from file with extractor ({0})")]
    ExtractFail(ExtractError),

    #[error("Failed to find any filesystems in the extracted contents")]
    FailToFind,
}

pub fn extract_and_process(
    extractor: &dyn Extractor,
    in_file: &Path,
    out_file_base: &Path,
    scratch_dir: Option<&Path>,
    verbose: bool,
    primary_limit: usize,
    secondary_limit: usize,
    results: &Mutex<Vec<ExtractionResult>>,
    metadata: &Metadata,
    removed_devices: Option<&Mutex<HashSet<PathBuf>>>,
) -> Result<(), ExtractProcessError> {
    let extractor_name = extractor.name();

    let scratch_dir = scratch_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(env::temp_dir);

    let temp_dir_prefix = format!("fw2tar_{extractor_name}");
    let temp_dir = TempDir::with_prefix_in(temp_dir_prefix, &scratch_dir)
        .map_err(ExtractProcessError::TempDirFail)?;

    let extract_dir = temp_dir.path();

    let log_file = {
        // Simple string append to avoid with_extension() being greedy
        let file_name = out_file_base.file_name().unwrap().to_string_lossy();
        out_file_base.with_file_name(format!("{}.{extractor_name}.log", file_name))
    };

    let start_time = Instant::now();

    extractor
        .extract(in_file, extract_dir, &log_file, verbose)
        .map_err(ExtractProcessError::ExtractFail)?;

    let elapsed = start_time.elapsed().as_secs_f32();

    if verbose {
        println!("{extractor_name} took {elapsed:.2} seconds")
    } else {
        log::info!("{extractor_name} took {elapsed:.2} seconds");
    }

    let rootfs_choices = find_linux_filesystems(extract_dir, None, extractor_name);

    if rootfs_choices.is_empty() {
        log::error!("No Linux filesystems found extracting {in_file:?} with {extractor_name}");
        return Err(ExtractProcessError::FailToFind);
    }

    // --- Primary filesystems ---
    for (i, fs) in rootfs_choices.iter().enumerate().take(primary_limit) {
        let tar_path = {
            // Simple string append to avoid with_extension() being greedy
            let file_name = out_file_base.file_name().unwrap().to_string_lossy();
            out_file_base.with_file_name(format!("{}.{extractor_name}.{i}.tar.gz", file_name))
        };

        // XXX: improve error handling here
        let file_node_count = tar_fs(&fs.path, &tar_path, metadata, removed_devices).unwrap();
        let archive_hash = sha1_file(&tar_path).unwrap();

        results.lock().unwrap().push(ExtractionResult {
            extractor: extractor_name,
            index: i,
            size: fs.size,
            num_files: fs.num_files,
            primary: true,
            archive_hash,
            file_node_count,
            path: tar_path,
            rootfs_score: fs.score,
        });
    }

    // --- Secondary filesystems ---
    // Archive additional rootfs candidates beyond the primary limit, up to secondary_limit more.
    // These are named .secondary.N.tar.gz and excluded from best-extractor selection in lib.rs.
    for (secondary_index, fs) in rootfs_choices
        .iter()
        .enumerate()
        .skip(primary_limit)
        .take(secondary_limit)
        .map(|(_, fs)| fs)
        .enumerate()
    {
        let tar_path = {
            let file_name = out_file_base.file_name().unwrap().to_string_lossy();
            out_file_base.with_file_name(format!(
                "{}.{extractor_name}.secondary.{secondary_index}.tar.gz",
                file_name
            ))
        };

        log::info!(
            "{extractor_name}: archiving secondary filesystem {secondary_index} at {}",
            fs.path.display()
        );

        let file_node_count = tar_fs(&fs.path, &tar_path, metadata, removed_devices).unwrap();
        let archive_hash = sha1_file(&tar_path).unwrap();

        results.lock().unwrap().push(ExtractionResult {
            extractor: extractor_name,
            index: primary_limit + secondary_index,
            size: fs.size,
            num_files: fs.num_files,
            primary: false,
            archive_hash,
            file_node_count,
            path: tar_path,
            rootfs_score: fs.score,
        });
    }

    // Warn if we found more filesystems than we were asked to archive.
    let total_limit = primary_limit + secondary_limit;
    if rootfs_choices.len() > total_limit {
        let skipped = rootfs_choices.len() - total_limit;
        println!(
            "WARNING: {extractor_name} found {total} filesystem(s) but only archived {total_limit}. \
             Skipping {skipped}. Increase --primary-limit or --secondary-limit to extract more.",
            total = rootfs_choices.len()
        );
    }

    drop(temp_dir);

    Ok(())
}

pub fn sha1_file(file: &Path) -> io::Result<String> {
    let bytes = std::fs::read(file)?;

    let mut hasher = Sha1::new();
    hasher.update(&bytes[..]);
    let result = hasher.finalize();

    Ok(format!("{result:x}"))
}
