use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::directory_executables::{get_dir_executable_info, ExecutableInfo};

const MAX_EXPLORE_DEPTH: usize = 15;

/// Minimum score to be considered a full primary rootfs candidate.
/// Calibrated to match the old `total_matches >= MIN_REQUIRED && executables >= 10` gate.
pub const PRIMARY_SCORE_MIN: f64 = 8.0;

/// Minimum score to be retained as a shard candidate for future merging phases.
pub const SHARD_SCORE_MIN: f64 = 3.0;

pub const KEY_DIRS: &[&str] = &["bin", "etc", "lib", "usr", "var"];
pub const CRITICAL_FILES: &[&str] = &["bin/sh", "etc/passwd"];

/// Continuous score for a candidate rootfs directory.
///
/// Weights:
/// - Each recognised key directory contributes 2.0 points.
/// - Each critical file (`bin/sh`, `etc/passwd`) contributes 3.0 points.
/// - Executables are counted on a log scale to avoid penalising stripped-down firmware
///   that happens to have very few executables while still rewarding richer ones.
///
/// Example break-even for PRIMARY_SCORE_MIN = 8.0:
///   3 key dirs (6.0) + 1 critical file (3.0) + 1 executable (ln(2) ≈ 0.69) → 9.69 ✓
///   2 key dirs (4.0) + 0 critical files + 10 executables (ln(11) ≈ 2.40) → 6.40 ✗
pub fn score(key_dirs: usize, critical_files: usize, executables: usize) -> f64 {
    (key_dirs as f64 * 2.0) + (critical_files as f64 * 3.0) + (1.0 + executables as f64).ln()
}

/// A candidate Linux rootfs directory, ranked by continuous score.
///
/// `is_primary` is `true` when `score >= PRIMARY_SCORE_MIN`.
/// Candidates with `SHARD_SCORE_MIN <= score < PRIMARY_SCORE_MIN` are retained as
/// shard candidates and can be merged in a later phase.
#[derive(Debug, Clone)]
pub struct ScoredFilesystem {
    pub path: PathBuf,
    pub size: u64,
    pub num_files: usize,
    pub key_dir_count: usize,
    pub critical_file_count: usize,
    pub executables: usize,
    pub score: f64,
    pub is_primary: bool,
}

pub fn find_linux_filesystems(
    start_dir: &Path,
    _min_executables: Option<usize>,
    extractor_name: &str,
) -> Vec<ScoredFilesystem> {
    let mut filesystems = Vec::new();

    log::info!("Searching {start_dir:?}");

    for entry in WalkDir::new(start_dir)
        .max_depth(MAX_EXPLORE_DEPTH)
        .into_iter()
        .filter_entry(|entry| entry.file_type().is_dir())
    {
        let Ok(entry) = entry else { continue };

        let root = entry.path();

        // Count key directories present.
        let key_dir_count = KEY_DIRS.iter().filter(|&&d| root.join(d).exists()).count();

        // Count critical files present.
        let critical_file_count = CRITICAL_FILES
            .iter()
            .filter(|&&f| root.join(f).exists())
            .count();

        // Pre-filter: skip directories that score below the shard minimum even in the
        // best case (all key dirs + all critical files, zero executables), to avoid
        // paying for the expensive `get_dir_executable_info` walk on obviously empty dirs.
        let max_possible = score(key_dir_count, critical_file_count, 0);
        if max_possible < SHARD_SCORE_MIN {
            if key_dir_count > 0 || critical_file_count > 0 {
                log::info!(
                    "Directory {} pre-filtered (max possible score {:.2} < {SHARD_SCORE_MIN})",
                    root.display(),
                    max_possible
                );
            }
            continue;
        }

        let ExecutableInfo {
            total_executables,
            total_size,
            total_files,
        } = get_dir_executable_info(root);

        let fs_score = score(key_dir_count, critical_file_count, total_executables);
        let is_primary = fs_score >= PRIMARY_SCORE_MIN;

        if fs_score >= SHARD_SCORE_MIN {
            log::info!(
                "{}: score={:.2} is_primary={} key_dirs={} critical_files={} executables={} size={}",
                root.display(),
                fs_score,
                is_primary,
                key_dir_count,
                critical_file_count,
                total_executables,
                total_size,
            );

            if !is_primary {
                log::info!(
                    "{extractor_name}: {} is a shard candidate (score {:.2} < {PRIMARY_SCORE_MIN})",
                    root.display(),
                    fs_score
                );
            }

            filesystems.push(ScoredFilesystem {
                path: root.to_owned(),
                size: total_size,
                num_files: total_files,
                key_dir_count,
                critical_file_count,
                executables: total_executables,
                score: fs_score,
                is_primary,
            });
        } else {
            log::warn!(
                "{extractor_name}: {} scored {:.2} (below shard minimum {SHARD_SCORE_MIN})",
                root.display(),
                fs_score
            );
        }
    }

    // Sort: primaries first, then shards; within each group sort by score descending.
    filesystems.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then_with(|| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| Reverse(b.size).cmp(&Reverse(a.size)))
    });

    filesystems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_key_dirs_sorted() {
        assert!(KEY_DIRS.is_sorted());
    }

    #[test]
    fn test_walkdir() {
        for entry in WalkDir::new(".") {
            let entry = entry.unwrap();
            println!("{}", entry.path().display());
        }
    }

    // --- Score function unit tests ---

    #[test]
    fn test_score_primary_typical() {
        // Typical full rootfs: 4 key dirs, 2 critical files, 15 executables
        // Expected: 4*2 + 2*3 + ln(16) ≈ 8 + 6 + 2.77 = 16.77
        let s = score(4, 2, 15);
        assert!(
            s >= PRIMARY_SCORE_MIN,
            "score {s:.2} should be >= PRIMARY_SCORE_MIN ({PRIMARY_SCORE_MIN})"
        );
        assert!(s > 16.0, "score {s:.2} should be roughly 16.77");
    }

    #[test]
    fn test_score_shard_partial() {
        // Partial rootfs: 1 key dir, 0 critical files, 2 executables
        // Expected: 1*2 + 0 + ln(3) ≈ 2 + 1.10 = 3.10
        let s = score(1, 0, 2);
        assert!(
            s >= SHARD_SCORE_MIN,
            "score {s:.2} should be >= SHARD_SCORE_MIN ({SHARD_SCORE_MIN})"
        );
        assert!(
            s < PRIMARY_SCORE_MIN,
            "score {s:.2} should be < PRIMARY_SCORE_MIN ({PRIMARY_SCORE_MIN})"
        );
    }

    #[test]
    fn test_score_below_shard() {
        // Near-empty: 0 key dirs, 0 critical files, 0 executables → ln(1) = 0.0
        let s = score(0, 0, 0);
        assert!(
            s < SHARD_SCORE_MIN,
            "score {s:.2} should be < SHARD_SCORE_MIN ({SHARD_SCORE_MIN})"
        );
    }

    #[test]
    fn test_score_primary_minimal() {
        // Minimum that should still count as primary:
        // 3 key dirs (6.0) + 1 critical file (3.0) + 1 executable (ln(2) ≈ 0.69) = 9.69
        let s = score(3, 1, 1);
        assert!(
            s >= PRIMARY_SCORE_MIN,
            "score {s:.2} should be >= PRIMARY_SCORE_MIN ({PRIMARY_SCORE_MIN})"
        );
    }

    #[test]
    fn test_score_executables_log_scale() {
        // Score must be strictly monotonically increasing with executables.
        let s1 = score(0, 0, 1);
        let s100 = score(0, 0, 100);
        let s10k = score(0, 0, 10_000);
        assert!(s1 < s100, "score should increase with more executables");
        assert!(s100 < s10k, "score should increase with more executables");

        // The log scale means the *marginal gain* per extra executable diminishes.
        // Going from 100→10_000 (+9_900 executables) should add less than
        // going from 1→100 (+99 executables) did, per unit of executables added.
        let rate_low = (s100 - s1) / 99.0;
        let rate_high = (s10k - s100) / 9_900.0;
        assert!(
            rate_high < rate_low,
            "log scale: marginal gain should diminish: {rate_low:.4} vs {rate_high:.4}"
        );
    }
}
