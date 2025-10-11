//! Test utilities to improve hygiene: always tear down containers and remove images.

use assert_cmd::Command;
use std::path::{Path, PathBuf};

/// Drop guard that will attempt to run `deacon down` for the given workspace
/// and remove any registered Docker images. All operations are best-effort.
pub struct DeaconGuard {
    workspace: PathBuf,
    image_ids: Vec<String>,
}

impl DeaconGuard {
    /// Create a new guard bound to a workspace folder.
    pub fn new(workspace: &Path) -> Self {
        Self {
            workspace: workspace.to_path_buf(),
            image_ids: Vec::new(),
        }
    }

    /// Register an image id to remove on drop.
    #[allow(dead_code)]
    pub fn register_image<S: Into<String>>(&mut self, image_id: S) {
        let id = image_id.into();
        if !id.is_empty() {
            self.image_ids.push(id);
        }
    }

    fn down_best_effort(&self) {
        if let Ok(mut cmd) = Command::cargo_bin("deacon") {
            let _ = cmd
                .current_dir(&self.workspace)
                .arg("down")
                .arg("--all")
                .arg("--volumes")
                .arg("--force")
                .assert()
                .get_output();
        }
    }

    fn remove_images_best_effort(&self) {
        for id in &self.image_ids {
            let _ = std::process::Command::new("docker")
                .arg("rmi")
                .arg("-f")
                .arg(id)
                .status();
        }
    }
}

impl Drop for DeaconGuard {
    fn drop(&mut self) {
        // Attempt to stop/remove containers first, then images.
        self.down_best_effort();
        self.remove_images_best_effort();
    }
}
