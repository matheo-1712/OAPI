//! Disk cleanup of generated profile images.
//!
//! Each stats change publishes a new PNG and earlier versions stay readable for links
//! already handed out. This task bounds that history: per owner directory it keeps the
//! [`KEEP_PER_OWNER`] newest images, plus anything younger than [`MIN_AGE`], and drops
//! temporary files left by an interrupted publication.

use crate::services::scheduler::TaskSummary;
use crate::utils::paths;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Newest images always kept per owner, whatever their age.
const KEEP_PER_OWNER: usize = 3;
/// Older images are only deleted once links to them are this old.
const MIN_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
/// A publication never takes this long: older temporary files are leftovers.
const TEMPORARY_AGE: Duration = Duration::from_secs(3600);

fn age(modified: SystemTime, now: SystemTime) -> Duration {
    now.duration_since(modified).unwrap_or_default()
}

/// Prunes one owner directory; returns how many files were deleted.
fn prune_owner(directory: &Path, now: SystemTime) -> io::Result<usize> {
    let mut images = Vec::new();
    let mut deleted = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".tmp") {
            if age(modified, now) >= TEMPORARY_AGE {
                std::fs::remove_file(entry.path())?;
                deleted += 1;
            }
        } else if name.ends_with(".png") {
            images.push((modified, entry.path()));
        }
    }
    images.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (modified, path) in images.into_iter().skip(KEEP_PER_OWNER) {
        if age(modified, now) >= MIN_AGE {
            std::fs::remove_file(path)?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// Prunes every owner directory under `root`; a missing root is an empty one.
pub fn prune_root(root: &Path, now: SystemTime, summary: &mut TaskSummary) -> io::Result<()> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        summary.processed += 1;
        let owner = entry.file_name().to_string_lossy().into_owned();
        match prune_owner(&entry.path(), now) {
            Ok(_) => summary.succeeded += 1,
            Err(error) => {
                tracing::warn!("Image cleanup of {owner} failed: {error}");
                summary.failed.push(owner);
            }
        }
    }
    Ok(())
}

/// Daily task: prunes Discord and Minecraft summary images.
pub async fn prune_generated_images() -> Result<TaskSummary, String> {
    tokio::task::spawn_blocking(|| {
        let now = SystemTime::now();
        let mut summary = TaskSummary::default();
        for root in [paths::DISCORD_SUMMARY_DIR, paths::MINECRAFT_SUMMARY_DIR] {
            prune_root(Path::new(root), now, &mut summary)
                .map_err(|e| format!("Unable to read {root}: {e}"))?;
        }
        Ok(summary)
    })
    .await
    .map_err(|e| format!("Image cleanup task failed: {e}"))?
}

/// Removes every generated image of a Discord account (its data is being purged).
pub async fn remove_discord_images(discord_id: &str) {
    remove_owner(Path::new(paths::DISCORD_SUMMARY_DIR), discord_id).await;
}

async fn remove_owner(root: &Path, owner: &str) {
    // The owner becomes a path segment: only plain snowflakes are accepted.
    if !crate::services::image_service::valid_discord_id(owner) {
        return;
    }
    match tokio::fs::remove_dir_all(root.join(owner)).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!("Unable to remove generated images: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let nonce = oauth2::CsrfToken::new_random();
        let directory = std::env::temp_dir().join(format!("oapi-{name}-{}", nonce.secret()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn touch(path: &Path, modified: SystemTime) {
        fs::write(path, b"png").unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn keeps_recent_and_newest_images_and_drops_old_leftovers() {
        let root = scratch("cleanup");
        let owner = root.join("123");
        fs::create_dir(&owner).unwrap();
        let now = SystemTime::now();
        let days = |n: u64| now - Duration::from_secs(n * 24 * 3600);
        for (name, when) in [
            ("a.png", days(30)),
            ("b.png", days(20)),
            ("c.png", days(15)),
            ("d.png", days(10)),
            ("e.png", days(1)),
            ("f.png", days(0)),
        ] {
            touch(&owner.join(name), when);
        }
        touch(&owner.join("g.png.x.tmp"), days(1));
        touch(&owner.join("h.png.y.tmp"), now);
        let mut summary = TaskSummary::default();
        prune_root(&root, now, &mut summary).unwrap();
        let mut left: Vec<String> = fs::read_dir(&owner)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // f, e, d are the newest three; c, b, a are older than seven days.
        assert_eq!(left, ["d.png", "e.png", "f.png", "h.png.y.tmp"]);
        assert_eq!((summary.processed, summary.succeeded), (1, 1));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_root_is_empty() {
        let mut summary = TaskSummary::default();
        prune_root(Path::new("does/not/exist"), SystemTime::now(), &mut summary).unwrap();
        assert_eq!(summary, TaskSummary::default());
    }

    #[tokio::test]
    async fn purged_accounts_lose_their_images_and_paths_stay_confined() {
        let root = scratch("purge-images");
        fs::create_dir(root.join("123")).unwrap();
        fs::write(root.join("123").join("a.png"), b"png").unwrap();
        fs::create_dir(root.join("keep")).unwrap();
        remove_owner(&root, "123").await;
        remove_owner(&root, "..").await;
        remove_owner(&root, "keep").await;
        remove_owner(&root, "456").await;
        assert!(!root.join("123").exists());
        assert!(root.join("keep").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
