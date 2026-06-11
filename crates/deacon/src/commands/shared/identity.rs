//! Canonical container-identity construction shared across subcommands.
//!
//! The `devcontainer.workspaceHash` + `devcontainer.configHash` labels that
//! `up` stamps on a container are how `exec`, `run-user-commands`, and `down`
//! find it again later (via [`ContainerIdentity::label_selector`]). For that
//! reconnection to work, every command MUST hash the **same** config.
//!
//! The single rule that keeps them in agreement (see issue #187): build the
//! identity from the configuration **exactly as loaded** — after extends
//! resolution, but BEFORE any of `up`'s runtime mutations (CLI `--mount` /
//! `--forward-ports`, image-metadata merge, feature merge, variable
//! substitution, and especially the Dockerfile build, which rewrites `build`
//! into a `deacon-build:<hash>` image). `exec`/`run-user-commands`/`down` never
//! replay those mutations, so a hash taken *after* them can never be
//! reproduced and reconnection silently breaks.
//!
//! Funnel reconnect-path identity construction through this helper so the
//! contract lives in one documented place and new call sites inherit it.

use std::path::Path;

use deacon_core::config::DevContainerConfig;
use deacon_core::container::ContainerIdentity;

/// Build the canonical reconnect [`ContainerIdentity`] for a workspace.
///
/// `config` MUST be the configuration *as loaded* (see module docs). `up`
/// passes the snapshot it takes immediately after `load_config`; `exec`,
/// `run-user-commands`, and `down` pass their freshly-loaded config (which they
/// never mutate), so every path agrees on `workspaceHash` / `configHash`.
///
/// `container_name` sets the optional custom container name and `config_file`
/// sets the `devcontainer.config_file` label. Both affect only the labels a
/// *creating* command stamps — they do not change the `workspaceHash` /
/// `configHash` used by the label selector — so commands that merely *resolve*
/// a container (e.g. `exec`) may pass `None`.
pub fn canonical_reconnect_identity(
    workspace_folder: &Path,
    config: &DevContainerConfig,
    container_name: Option<String>,
    config_file: Option<&Path>,
) -> ContainerIdentity {
    let identity =
        ContainerIdentity::new_with_custom_name(workspace_folder, config, container_name);
    match config_file {
        Some(path) => identity.with_config_file(path),
        None => identity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample_config() -> DevContainerConfig {
        serde_json::from_str(r#"{ "name": "demo", "image": "alpine:3.18" }"#).unwrap()
    }

    #[test]
    fn matches_plain_constructor_when_no_name_or_config_file() {
        let ws = Path::new("/tmp/demo-ws");
        let config = sample_config();

        let helper = canonical_reconnect_identity(ws, &config, None, None);
        let plain = ContainerIdentity::new(ws, &config);

        // The reconnect-critical fields must be identical: this is the whole
        // contract that lets exec/run-user-commands/down find up's container.
        assert_eq!(helper.workspace_hash, plain.workspace_hash);
        assert_eq!(helper.config_hash, plain.config_hash);
        assert_eq!(helper.label_selector(), plain.label_selector());
    }

    #[test]
    fn custom_name_and_config_file_do_not_perturb_the_selector() {
        let ws = Path::new("/tmp/demo-ws");
        let config = sample_config();

        let bare = canonical_reconnect_identity(ws, &config, None, None);
        let decorated = canonical_reconnect_identity(
            ws,
            &config,
            Some("my-name".to_string()),
            Some(Path::new("/tmp/demo-ws/.devcontainer/devcontainer.json")),
        );

        // Name + config_file change only the labels a creating command stamps;
        // the selector (source + workspaceHash + configHash) is unchanged, so a
        // resolver passing None still matches a creator passing Some.
        assert_eq!(bare.label_selector(), decorated.label_selector());
        assert_eq!(decorated.custom_name.as_deref(), Some("my-name"));
    }
}
