//! The reference CLI's third substitution pass (`containerSubstitute`) over the
//! configuration a subcommand REPORTS.
//!
//! Every consumer subcommand that PRINTS a configuration alongside a live container
//! owes this pass, and each one of them got it late: `up` in #608/#613, `set-up` in
//! #616. The gap is always the same shape — the document was serialized from the
//! pass-1 configuration, which by construction predates (or ignores) the container
//! and therefore cannot have resolved a single `${containerEnv:*}` — so the helper
//! lives here rather than inside either command.
//!
//! Two entry points, because the two commands disagree about the WORKSPACE, not
//! about the container:
//!
//! - [`container_substituted_config`] — `up`'s. It has a `--workspace-folder` and a
//!   container identity, so it builds the context itself.
//! - [`container_substituted_with_context`] — `set-up`'s. It has neither, and its
//!   context is deliberately built by `SubstitutionContext::without_workspace` so
//!   `${localWorkspaceFolder}`, `${localWorkspaceFolderBasename}` and
//!   `${devcontainerId}` stay LITERAL (#510) and `${containerWorkspaceFolder}` comes
//!   from the `--config` document's own `workspaceFolder` (#513). Handing it the
//!   workspace-anchored constructor would silently undo both, so it passes the
//!   context it already built and this adds only the container environment.

use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Docker;
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

/// Re-run variable substitution against the live container, so the document the
/// caller is HANDED resolves the container-aware tokens. `up`'s entry point.
///
/// This is the reference CLI's third substitution pass (`containerSubstitute`,
/// `variableSubstitution.ts`). deacon already ran it for everything that USES the
/// configuration at runtime — `resolve_env_and_user` resolves `${containerEnv:*}`
/// in `remoteEnv` before injecting it, which is why `deacon exec` on the same
/// container has the right values — but the `--include-configuration` /
/// `--include-merged-configuration` blocks were serialized from the pass-1
/// configuration, which by construction predates the container and therefore
/// cannot have resolved a single `${containerEnv:*}`. Issue #608.
///
/// `container_env` is the RAW container environment (`inspect`'s `Config.Env`),
/// the canonical source for `${containerEnv:VAR}` — not the userEnvProbe result
/// and not the merged effective env.
///
/// Fail-safe: when the container environment could not be read, the caller passes
/// `None` and this returns the configuration untouched, so the template survives
/// instead of collapsing to an empty string (`resolve_variable` returns
/// `Some("")` for a missing key once `container_env` is `Some`).
pub(crate) fn container_substituted_config(
    config: &DevContainerConfig,
    workspace_folder: &Path,
    devcontainer_id: &str,
    container_env: Option<&HashMap<String, String>>,
    container_workspace_folder: &str,
) -> DevContainerConfig {
    let Some(container_env) = container_env else {
        debug!(
            "No container environment available; reporting configuration as substituted pre-container"
        );
        return config.clone();
    };

    let mut context = match SubstitutionContext::new(workspace_folder) {
        Ok(context) => context,
        Err(error) => {
            warn!(
                "Could not build a container substitution context for the reported configuration: {}",
                error
            );
            return config.clone();
        }
    };
    context.devcontainer_id = devcontainer_id.to_string();
    context.container_workspace_folder = Some(container_workspace_folder.to_string());

    apply_container_pass(config, &context, container_env)
}

/// The same pass over a context the CALLER already built. `set-up`'s entry point.
///
/// `set-up` adopts a container it did not create and takes no `--workspace-folder`,
/// so its context is `SubstitutionContext::without_workspace` with
/// `container_workspace_folder` read off the `--config` document (#510/#513). Every
/// one of those decisions is a measured characterization, so the container pass adds
/// the ONE thing pass 1 could not know — the container's environment — and changes
/// nothing else about the context.
///
/// Fail-safe is identical to [`container_substituted_config`]: `None` returns the
/// configuration untouched rather than resolving every `${containerEnv:*}` to the
/// empty string.
pub(crate) fn container_substituted_with_context(
    config: &DevContainerConfig,
    base_context: &SubstitutionContext,
    container_env: Option<&HashMap<String, String>>,
) -> DevContainerConfig {
    let Some(container_env) = container_env else {
        debug!(
            "No container environment available; reporting configuration as substituted pre-container"
        );
        return config.clone();
    };

    apply_container_pass(config, base_context, container_env)
}

/// Clone the caller's context, add the container environment, and re-substitute.
fn apply_container_pass(
    config: &DevContainerConfig,
    base_context: &SubstitutionContext,
    container_env: &HashMap<String, String>,
) -> DevContainerConfig {
    let mut context = base_context.clone();
    context.container_env = Some(container_env.clone());

    let (substituted, report) = config.apply_variable_substitution(&context);
    debug!(
        "Container substitution pass over the reported configuration made {} replacements",
        report.replacements.len()
    );
    substituted
}

/// Read the raw container environment for the container substitution pass.
///
/// Best-effort by design: an inspect failure yields `None`, which leaves the
/// reported configuration exactly as it was rather than resolving every
/// `${containerEnv:*}` to an empty string.
pub(crate) async fn container_env_for_substitution<D: Docker>(
    runtime: &D,
    container_id: &str,
) -> Option<HashMap<String, String>> {
    match runtime.inspect_container(container_id).await {
        Ok(Some(info)) => Some(info.env),
        Ok(None) => {
            warn!(
                "Container '{}' not found while resolving the reported configuration",
                container_id
            );
            None
        }
        Err(error) => {
            warn!(
                "Container inspect failed while resolving the reported configuration: {}",
                error
            );
            None
        }
    }
}
