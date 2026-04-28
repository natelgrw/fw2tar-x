pub mod signatures;

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

    // Very naive multi-pattern string search for Phase 4.
    // Iterating byte by byte for simplicity; Phase 6 will replace this with Aho-Corasick.
    for i in 0..mmap.len() {
        for sig in signatures::SIGNATURES {
            if i + sig.magic.len() <= mmap.len() && &mmap[i..i + sig.magic.len()] == sig.magic {
                regions.push(DetectedRegion {
                    offset: i as u64,
                    signature_type: sig.name.to_string(),
                });
            }
        }

        for (name, offset, magic) in signatures::SPECIAL_SIGNATURES {
            if i + magic.len() <= mmap.len() && &mmap[i..i + magic.len()] == *magic {
                if i >= *offset {
                    regions.push(DetectedRegion {
                        offset: (i - *offset) as u64,
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
