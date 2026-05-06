pub mod signatures;
pub mod validation;

use memmap2::MmapOptions;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedRegion {
    pub offset: u64,
    pub signature_type: String,
}

pub fn scan_firmware(file_path: &Path) -> io::Result<Vec<DetectedRegion>> {
    let file = File::open(file_path)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let mut regions = Vec::new();

    let mut patterns = Vec::new();
    for sig in signatures::SIGNATURES {
        patterns.push(sig.magic);
    }
    for (_, _, magic) in signatures::SPECIAL_SIGNATURES {
        patterns.push(*magic);
    }

    let ac = aho_corasick::AhoCorasick::new(&patterns).unwrap();

    for mat in ac.find_overlapping_iter(&*mmap) {
        let pattern_idx = mat.pattern().as_usize();
        let i = mat.start();

        if pattern_idx < signatures::SIGNATURES.len() {
            let sig = &signatures::SIGNATURES[pattern_idx];
            let is_valid = match sig.name {
                "squashfs" => validation::validate_squashfs(&mmap, i),
                "jffs2" => validation::validate_jffs2(&mmap, i),
                _ => true,
            };
            
            if is_valid {
                regions.push(DetectedRegion {
                    offset: i as u64,
                    signature_type: sig.name.to_string(),
                });
            }
        } else {
            let special_idx = pattern_idx - signatures::SIGNATURES.len();
            let (name, offset, _) = &signatures::SPECIAL_SIGNATURES[special_idx];
            if i >= *offset {
                let start_offset = i - *offset;
                let is_valid = match *name {
                    "ext" => validation::validate_ext(&mmap, start_offset),
                    _ => true,
                };
                
                if is_valid {
                    regions.push(DetectedRegion {
                        offset: start_offset as u64,
                        signature_type: name.to_string(),
                    });
                }
            }
        }
    }

    // Since the SPECIAL_SIGNATURES logic above will repeatedly push the same region once `i`
    // advances past `start`, let's just do a deduplication here to be safe and clean.
    regions.dedup_by(|a, b| a.offset == b.offset && a.signature_type == b.signature_type);

    Ok(regions)
}
