use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::extractors::{ExtractError, Extractor};
use crate::scanner;
use memmap2::MmapOptions;

pub struct NativeExtractor;

impl Extractor for NativeExtractor {
    fn name(&self) -> &'static str {
        "native"
    }

    fn extract(
        &self,
        in_file: &Path,
        extract_dir: &Path,
        _log_file: &Path,
        verbose: bool,
    ) -> Result<(), ExtractError> {
        let regions = scanner::scan_firmware(in_file).unwrap_or_default();
        if regions.is_empty() {
            return Ok(());
        }

        let file = fs::File::open(in_file)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };

        for region in regions {
            let offset = region.offset as usize;
            if offset >= mmap.len() {
                continue;
            }

            // Create a sub-directory for this extracted region
            let dest_dir = extract_dir.join(format!("{}_{}", region.signature_type, offset));
            fs::create_dir_all(&dest_dir)?;

            // Fast native carving: Write the carved bytes to a temp file
            let carve_path = extract_dir.join(format!("carved_{}_{}.bin", region.signature_type, offset));
            let mut carve_file = fs::File::create(&carve_path)?;
            carve_file.write_all(&mmap[offset..])?;

            let mut cmd = match region.signature_type.as_str() {
                "squashfs" => {
                    let mut c = Command::new("unsquashfs");
                    c.arg("-f").arg("-d").arg(&dest_dir).arg(&carve_path);
                    c
                }
                "jffs2" => {
                    let mut c = Command::new("jefferson");
                    c.arg("-d").arg(&dest_dir).arg(&carve_path);
                    c
                }
                "cpio" => {
                    // cpio needs to read from stdin
                    let mut c = Command::new("cpio");
                    c.arg("-idm").current_dir(&dest_dir);
                    c
                }
                "gzip" => {
                    let mut c = Command::new("tar");
                    c.arg("-xzf").arg(&carve_path).arg("-C").arg(&dest_dir);
                    c
                }
                "xz" => {
                    let mut c = Command::new("tar");
                    c.arg("-xJf").arg(&carve_path).arg("-C").arg(&dest_dir);
                    c
                }
                _ => {
                    // Cleanup unhandled carve
                    let _ = fs::remove_file(&carve_path);
                    continue;
                }
            };

            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

            if region.signature_type == "cpio" {
                // Pipe carve_file to cpio
                cmd.stdin(fs::File::open(&carve_path)?);
            }

            match cmd.spawn() {
                Ok(mut child) => {
                    let timeout = super::get_timeout();
                    match child.wait_timeout(timeout) {
                        Ok(Some(status)) => {
                            if !status.success() && verbose {
                                log::error!("Native extractor {} failed at offset {}", region.signature_type, offset);
                            }
                        }
                        Ok(None) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            log::warn!("Native extractor {} timed out at offset {}", region.signature_type, offset);
                        }
                        Err(e) => {
                            log::error!("Error waiting for {}: {}", region.signature_type, e);
                        }
                    }
                }
                Err(e) => {
                    if verbose {
                        log::error!("Failed to spawn native tool for {}: {}", region.signature_type, e);
                    }
                }
            }

            // Cleanup carved file to save space
            let _ = fs::remove_file(&carve_path);
        }

        Ok(())
    }
}
