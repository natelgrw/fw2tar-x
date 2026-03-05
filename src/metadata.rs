use serde::{Deserialize, Serialize};

/// Output archive metadata that is concatonated to the tar (inside the gzip)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub input_hash: String,
    pub file: String,
    pub fw2tar_command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareMetadata {
    pub input_hash: String,
    pub file: String,
    pub image_size: u64,
    pub fw2tar_command: Vec<String>,
    pub archives: Vec<ArchiveMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    pub path: String,
    pub extractor: String,
    pub rootfs_score: f64,
    pub was_merged: bool,
    pub file_node_count: usize,
    pub archive_hash: String,
}
