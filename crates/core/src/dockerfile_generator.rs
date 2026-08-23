//! Dockerfile generation for feature installation using BuildKit
//!
//! This module generates Dockerfiles that install DevContainer features during
//! the image build phase using Docker BuildKit's mount capabilities. This approach
//! provides proper layer caching and follows the DevContainer specification.

use crate::build::BuildOptions;
use crate::errors::{FeatureError, Result};
use crate::features::{InstallationPlan, OptionValue, ResolvedFeature};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Build context name for feature content source
/// This name is used in both the Dockerfile generation and build arguments
const FEATURE_CONTENT_SOURCE: &str = "dev_containers_feature_content_source";

/// Build context name carrying the corporate-CA bundle + install script for
/// build-time host-CA injection (016). Mirrors [`FEATURE_CONTENT_SOURCE`].
pub const HOST_CA_BUILD_CONTEXT: &str = "deacon_ca_source";

/// In-build mount target for [`HOST_CA_BUILD_CONTEXT`]. The context dir holds
/// `host-ca.crt` (the PEM bundle) and `install.sh` (the shared install script).
/// Public so the caller staging the context references the same path.
pub const HOST_CA_MOUNT_TARGET: &str = "/tmp/deacon-ca";

/// The deterministic CA-install `RUN` step emitted before the feature loop when
/// build-time host-CA injection is enabled. Byte-stable text; the per-build
/// determinism + cache-busting comes from the mounted bundle/script content
/// (BuildKit hashes the bind mount), exactly like the feature mounts.
fn host_ca_run_step() -> String {
    format!(
        "# Host-CA injection (016): install the corporate CA before any feature install.\n\
         RUN --mount=type=bind,from={ctx},target={target} \\\n    \
         sh {target}/install.sh\n\n",
        ctx = HOST_CA_BUILD_CONTEXT,
        target = HOST_CA_MOUNT_TARGET,
    )
}

/// Configuration for Dockerfile generation
#[derive(Debug, Clone)]
pub struct DockerfileConfig {
    /// Base image to extend
    pub base_image: String,
    /// Target stage name
    pub target_stage: String,
    /// Directory where features are downloaded on the host
    pub features_source_dir: String,
    /// Spec-required env vars surfaced to every feature's `install.sh`
    /// (`_REMOTE_USER`, `_REMOTE_USER_HOME`, `_CONTAINER_USER`,
    /// `_CONTAINER_USER_HOME`). Populated by the caller from the resolved
    /// devcontainer.json. Per the [features spec](https://containers.dev/implementors/features/#installation-environment)
    /// these MUST be available; missing ones default to empty strings so
    /// install scripts can branch on `${_REMOTE_USER:-}` (#89).
    pub feature_install_env: FeatureInstallEnv,
    /// When `Some(dir)`, build-time host-CA injection is enabled (016): the
    /// generator emits a deterministic CA-install `RUN` step before the feature
    /// loop, and [`DockerfileGenerator::generate_build_args`] passes
    /// `--build-context deacon_ca_source=<dir>`. `dir` holds `host-ca.crt` and
    /// `install.sh`. `None` ⇒ no build-time injection (output unchanged).
    pub host_ca_build_context: Option<String>,
    /// The CONFIGURATION's own `containerEnv`, baked into the produced image as
    /// `ENV` lines emitted LAST — after every Feature's install `RUN` and after
    /// every Feature's own `containerEnv` (#641).
    ///
    /// Last is the whole point: it is what makes the configuration win where a
    /// Feature declares the same name. Without it a `build` hands out an image
    /// missing the variables its author configured and carrying a Feature's
    /// value for the ones they overrode — invisible in the result document,
    /// since `outcome: success` is unaffected.
    ///
    /// Authored order is preserved (`IndexMap`), matching the reference CLI's
    /// `Object.keys` iteration.
    pub config_container_env: IndexMap<String, String>,
}

/// The four well-known env vars the features spec guarantees to every
/// `install.sh`. See [`DockerfileConfig::feature_install_env`].
#[derive(Debug, Clone, Default)]
pub struct FeatureInstallEnv {
    /// `_REMOTE_USER` — the user lifecycle commands run as.
    pub remote_user: Option<String>,
    /// `_REMOTE_USER_HOME` — that user's home directory.
    pub remote_user_home: Option<String>,
    /// `_CONTAINER_USER` — the container's default user.
    pub container_user: Option<String>,
    /// `_CONTAINER_USER_HOME` — that user's home directory.
    pub container_user_home: Option<String>,
}

impl FeatureInstallEnv {
    /// Resolve the four env vars from the user-provided config values.
    ///
    /// Spec rules (mirrors upstream `@devcontainers/cli`):
    /// - `_REMOTE_USER` defaults to `_CONTAINER_USER` when not specified.
    /// - `_CONTAINER_USER` defaults to the image's `USER` (caller is
    ///   responsible for passing it in via `image_user`; we never assume
    ///   `"root"` here so callers that can't inspect the image surface
    ///   the gap rather than guessing).
    /// - `_*_HOME` defaults to `/root` for `root`, `/home/<user>`
    ///   otherwise. Callers that know the actual home dir from
    ///   `getent passwd` may pass it directly.
    pub fn resolve(
        remote_user: Option<&str>,
        container_user: Option<&str>,
        image_user: Option<&str>,
    ) -> Self {
        let container = container_user.or(image_user);
        let remote = remote_user.or(container);

        let home_for = |u: Option<&str>| -> Option<String> {
            u.map(|name| {
                if name == "root" {
                    "/root".to_string()
                } else {
                    format!("/home/{}", name)
                }
            })
        };

        Self {
            remote_user: remote.map(String::from),
            remote_user_home: home_for(remote),
            container_user: container.map(String::from),
            container_user_home: home_for(container),
        }
    }
}

impl Default for DockerfileConfig {
    fn default() -> Self {
        Self {
            base_image: String::new(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: String::new(),
            feature_install_env: FeatureInstallEnv::default(),
            host_ca_build_context: None,
            config_container_env: IndexMap::new(),
        }
    }
}

/// Generates a Dockerfile for installing features using BuildKit
#[derive(Debug)]
pub struct DockerfileGenerator {
    config: DockerfileConfig,
}

impl DockerfileGenerator {
    /// Create a new Dockerfile generator
    pub fn new(config: DockerfileConfig) -> Self {
        Self { config }
    }

    /// Generate a complete Dockerfile for feature installation
    #[instrument(skip(self, plan))]
    pub fn generate(&self, plan: &InstallationPlan) -> Result<String> {
        debug!(
            "Generating Dockerfile for {} features across {} levels",
            plan.len(),
            plan.levels.len()
        );

        let mut dockerfile = String::new();

        // Build argument for base image
        dockerfile.push_str(&format!(
            "ARG _DEV_CONTAINERS_BASE_IMAGE={}\n\n",
            self.config.base_image
        ));

        // FROM stage
        dockerfile.push_str(&format!(
            "FROM ${{_DEV_CONTAINERS_BASE_IMAGE}} AS {}\n\n",
            self.config.target_stage
        ));

        // Create temporary directory for features
        dockerfile.push_str("RUN mkdir -p /tmp/dev-container-features\n\n");

        // Host-CA injection (016, T037): install the corporate CA into the
        // distro trust store BEFORE any feature `install.sh` RUN-mount, so
        // feature network calls trust the proxy CA. Deterministic + cache-keyed
        // on the mounted bundle content (FR-017).
        if self.config.host_ca_build_context.is_some() {
            dockerfile.push_str(&host_ca_run_step());
        }

        // Install features level by level
        let mut install_index = 0usize;
        for (level_idx, level) in plan.levels.iter().enumerate() {
            dockerfile.push_str(&format!("# Level {}: Installing features\n", level_idx));

            for &feature_index in level {
                let feature =
                    plan.feature_at(feature_index)
                        .ok_or_else(|| FeatureError::NotFound {
                            path: format!("Feature #{} in installation plan", feature_index),
                        })?;

                dockerfile.push_str(&self.generate_feature_install_command(
                    feature,
                    level_idx,
                    install_index,
                )?);
                install_index += 1;
            }

            dockerfile.push('\n');
        }

        dockerfile.push_str(&self.config_container_env_lines());

        Ok(dockerfile)
    }

    /// Generate just the feature-install body (no `FROM` line, no `ARG` line)
    /// targeting `target_stage`. Used by the compose `build:` flow where the
    /// caller has already produced the upstream stages from a user-authored
    /// Dockerfile and only needs the install layers appended.
    ///
    /// Emits a self-contained stage:
    /// ```dockerfile
    /// FROM <base_stage_name> AS <self.config.target_stage>
    /// RUN mkdir -p /tmp/dev-container-features
    /// # Level 0: ...
    /// RUN --mount=type=bind,from=dev_containers_feature_content_source,... ./install.sh
    /// ```
    ///
    /// Unlike [`generate`], this does NOT use an ARG-driven base image — the
    /// stage name is written literally so BuildKit can resolve it against an
    /// earlier stage in the same Dockerfile (ARG/build-arg substitution in
    /// FROM only works when the ARG is declared in the global preamble, which
    /// is not possible when we append to a user Dockerfile that has its own
    /// FROM stages first).
    #[instrument(skip(self, plan))]
    pub fn generate_install_stage_from(
        &self,
        plan: &InstallationPlan,
        base_stage_name: &str,
    ) -> Result<String> {
        let mut dockerfile = String::new();

        dockerfile.push_str(&format!(
            "FROM {} AS {}\n\n",
            base_stage_name, self.config.target_stage
        ));

        dockerfile.push_str("RUN mkdir -p /tmp/dev-container-features\n\n");

        // Host-CA injection (016, T037): install the corporate CA into the
        // distro trust store BEFORE any feature `install.sh` RUN-mount, so
        // feature network calls trust the proxy CA. Deterministic + cache-keyed
        // on the mounted bundle content (FR-017).
        if self.config.host_ca_build_context.is_some() {
            dockerfile.push_str(&host_ca_run_step());
        }

        let mut install_index = 0usize;
        for (level_idx, level) in plan.levels.iter().enumerate() {
            dockerfile.push_str(&format!("# Level {}: Installing features\n", level_idx));

            for &feature_index in level {
                let feature =
                    plan.feature_at(feature_index)
                        .ok_or_else(|| FeatureError::NotFound {
                            path: format!("Feature #{} in installation plan", feature_index),
                        })?;

                dockerfile.push_str(&self.generate_feature_install_command(
                    feature,
                    level_idx,
                    install_index,
                )?);
                install_index += 1;
            }

            dockerfile.push('\n');
        }

        dockerfile.push_str(&self.config_container_env_lines());

        Ok(dockerfile)
    }

    /// The configuration's own `containerEnv` as `ENV` lines, emitted at the very
    /// end of the Feature-install stage by both entry points (#641).
    ///
    /// Position and escaping are both load-bearing, and they differ from the
    /// per-Feature `ENV` lines in [`Self::generate_feature_install_command`]:
    ///
    /// - **Last.** A Feature's `ENV` sits immediately after its own install
    ///   `RUN`, so it is visible to later installs. The configuration's comes
    ///   after all of them, which is what makes the configuration's value win
    ///   where both declare a name — the reference CLI orders its template the
    ///   same way, for the same reason.
    /// - **`$` is escaped here and deliberately is not there.** A Feature's
    ///   value legitimately self-references the environment it is extending
    ///   (`PATH="/usr/local/share/nvm/current/bin:${PATH}"`) and must go through
    ///   Docker's `ENV` expansion. A configuration's value is meant literally:
    ///   `"value with $dollar sign"` must reach the image with the `$` intact
    ///   rather than expanding to nothing.
    ///
    /// Emits an empty string when the configuration declares no `containerEnv`,
    /// so a build's Dockerfile is byte-identical to what it was before this
    /// existed.
    fn config_container_env_lines(&self) -> String {
        let mut lines = String::new();
        for (key, value) in &self.config.config_container_env {
            lines.push_str(&format!(
                "ENV {}\n",
                Self::format_env_var_literal(key, value)
            ));
        }
        lines
    }

    /// Generate the RUN command for installing a single feature.
    ///
    /// Feature option env vars are passed via `export KEY="value"` followed
    /// by `&& cd ... && ./install.sh`. The previous form (inline
    /// `KEY="value" cd …`) only exported the variables to `cd` itself —
    /// every subsequent `&&`-chained command (including `./install.sh`,
    /// which is what actually consumes the options) ran with empty env
    /// (#88). `export … &&` propagates the variables to every later
    /// command in the chain.
    fn generate_feature_install_command(
        &self,
        feature: &ResolvedFeature,
        level_idx: usize,
        install_index: usize,
    ) -> Result<String> {
        let feature_dir_name = Self::feature_staging_dir_name(feature, install_index);
        let mount_target = format!("/tmp/build-features-{}/{}", level_idx, feature_dir_name);

        let mut command = String::new();

        // Start RUN command with BuildKit mount
        command.push_str(&format!(
            "RUN --mount=type=bind,from={},source={},target={},rw \\\n",
            FEATURE_CONTENT_SOURCE, feature_dir_name, mount_target
        ));

        // Export environment variables for feature options so they
        // propagate to the install script (and any other command in the
        // chain). Deterministic order: sort by sanitized key.
        let env_vars = Self::build_environment_variables(feature);
        if !env_vars.is_empty() {
            let mut sorted: Vec<(&String, &String)> = env_vars.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            for (key, value) in sorted {
                command.push_str(&format!(
                    "    export {} && \\\n",
                    Self::format_env_var(key, value)
                ));
            }
        }

        // Spec-required env vars surfaced to every install.sh (#89). These
        // four are guaranteed by the features spec regardless of whether
        // the feature itself declares them in `options`. Use the same
        // `export … &&` form so they propagate through the chain alongside
        // the option env vars. Empty values are still emitted so
        // `${_REMOTE_USER:-}` in install.sh resolves to the empty string
        // rather than the literal `<unset>`.
        let install_env = &self.config.feature_install_env;
        for (key, value) in [
            ("_REMOTE_USER", install_env.remote_user.as_deref()),
            ("_REMOTE_USER_HOME", install_env.remote_user_home.as_deref()),
            ("_CONTAINER_USER", install_env.container_user.as_deref()),
            (
                "_CONTAINER_USER_HOME",
                install_env.container_user_home.as_deref(),
            ),
        ] {
            command.push_str(&format!(
                "    export {} && \\\n",
                Self::format_env_var(key, value.unwrap_or(""))
            ));
        }

        // Execute the install script
        command.push_str(&format!(
            "    cd {} && chmod +x install.sh && ./install.sh\n",
            mount_target
        ));

        // Persist the feature's `containerEnv` as Dockerfile `ENV` lines AFTER
        // its install step, so it survives into every subsequent feature's
        // install `RUN` and into the final image — matching the upstream
        // devcontainer CLI. Without this, a feature like `node` (which installs
        // Node via nvm and exposes it only through
        // `containerEnv.PATH = "/usr/local/share/nvm/current/bin:${PATH}"`)
        // leaves `npm`/`node` invisible to a later feature's `install.sh`,
        // causing `npm: command not found`. The `export … &&` chain above is
        // scoped to a single RUN and cannot cross feature boundaries; `ENV`
        // does. Values are emitted verbatim (only `\`/`"` escaped) so Docker's
        // own `ENV` expansion resolves `${PATH}`-style self-references against
        // the accumulated environment. Deterministic order: sort by key.
        if !feature.metadata.container_env.is_empty() {
            let mut sorted: Vec<(&String, &String)> =
                feature.metadata.container_env.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            for (key, value) in sorted {
                command.push_str(&format!("ENV {}\n", Self::format_env_var(key, value)));
            }
        }

        command.push('\n');

        Ok(command)
    }

    /// The directory name a feature's content is staged into, both on the host
    /// build context and as the BuildKit bind-mount `source=`.
    ///
    /// `<sanitized feature id>_<index>`, where `index` is the feature's position
    /// in the resolved INSTALL ORDER (`InstallationPlan::features`, which is the
    /// flattened `levels`). The index used to be the parallel LEVEL, which meant
    /// two features installed in the same round shared a suffix and the name
    /// carried no position information; a running index is unique by
    /// construction and is also what the reference CLI names its staged
    /// directories (`consecutiveId`), so `read-configuration` can report the
    /// path `up` will actually stage into.
    ///
    /// The id is the FEATURE'S OWN id from its `devcontainer-feature.json`
    /// (`go`, `common-utils`, `needs-node`) rather than `ResolvedFeature::id`,
    /// which is a per-command canonical form — the registry path in `up`/`build`
    /// and the short metadata id in `read-configuration`. Keying on the metadata
    /// id is what lets those two commands agree on one name. A feature whose
    /// metadata carries no id falls back to the canonical id rather than
    /// producing a bare `_<index>`.
    pub fn feature_staging_dir_name(feature: &ResolvedFeature, index: usize) -> String {
        let id = if feature.metadata.id.is_empty() {
            feature.id.as_str()
        } else {
            feature.metadata.id.as_str()
        };
        format!("{}_{}", Self::sanitize_feature_id(id), index)
    }

    /// Sanitize feature ID for use in file paths.
    ///
    /// Public so consumers that need to reverse-map a build-log mount marker
    /// (`source=<sanitized_id>_<index>`) back to a feature — e.g. the build
    /// progress parser in the `deacon` crate — can compute the same sanitized
    /// form instead of re-implementing (and drifting from) this scheme.
    pub fn sanitize_feature_id(id: &str) -> String {
        // Replace special characters with underscores
        id.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Build environment variables from feature options.
    ///
    /// Per the [features spec](https://containers.dev/implementors/features/#options-environment-vars),
    /// option ids are passed to `install.sh` as env vars. Shells reject env
    /// var names containing anything but `[A-Za-z_][A-Za-z0-9_]*`, so deacon
    /// MUST sanitize: uppercase the key, then replace every non-alphanumeric
    /// non-underscore character with `_`. Without this the BuildKit `RUN`
    /// step crashes on options like `another.weird-key`:
    ///
    /// ```text
    /// /bin/sh: ANOTHER.WEIRD-KEY=x/y/z: not found
    /// ```
    ///
    /// Option *values* are never sanitized — only keys (#88).
    fn build_environment_variables(feature: &ResolvedFeature) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();

        // The Feature's own declared defaults first, then the options the user (or the
        // `dependsOn` entry) actually authored, which win.
        //
        // Defaults are materialized HERE rather than in `ResolvedFeature.options` because
        // that map is half the Feature's IDENTITY (#489): `feature-dependencies.md`
        // §Definition: Feature Equality compares the options a Feature "is executed with"
        // as AUTHORED, so pre-filling defaults there would make two differently-authored
        // instances compare equal (and would order them by their defaults).
        for (name, definition) in &feature.metadata.options {
            if let Some(default_value) = definition.default_value() {
                env_vars.insert(
                    Self::sanitize_option_key(name),
                    Self::option_value_to_string(&default_value),
                );
            }
        }

        for (key, value) in &feature.options {
            let env_key = Self::sanitize_option_key(key);
            let env_value = Self::option_value_to_string(value);
            env_vars.insert(env_key, env_value);
        }

        env_vars
    }

    /// Sanitize a feature option id into a valid POSIX shell env var name.
    ///
    /// - Uppercase per the spec.
    /// - Replace any `[^A-Z0-9_]` character with `_` so non-identifier
    ///   characters like `.` and `-` don't reach `/bin/sh`.
    /// - Leave purely numeric / empty keys to the caller — the dependency
    ///   resolver upstream guarantees ids are non-empty.
    fn sanitize_option_key(id: &str) -> String {
        id.to_uppercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Convert OptionValue to string for environment variable
    fn option_value_to_string(value: &OptionValue) -> String {
        match value {
            OptionValue::Boolean(b) => b.to_string(),
            OptionValue::String(s) => s.clone(),
            OptionValue::Number(n) => n.to_string(),
            OptionValue::Array(a) => serde_json::to_string(a).unwrap_or_default(),
            OptionValue::Object(o) => serde_json::to_string(o).unwrap_or_default(),
            OptionValue::Null => String::new(),
        }
    }

    /// Format environment variable for Dockerfile
    fn format_env_var(key: &str, value: &str) -> String {
        // Escape special characters in value
        let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{}=\"{}\"", key, escaped_value)
    }

    /// Like [`Self::format_env_var`] but also escapes `$`, so the value reaches
    /// the image LITERALLY instead of going through Docker's `ENV` expansion.
    ///
    /// Used for the configuration's own `containerEnv` (#641). `\` is escaped
    /// first so the backslashes this inserts are not themselves doubled.
    fn format_env_var_literal(key: &str, value: &str) -> String {
        let escaped_value = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$");
        format!("{}=\"{}\"", key, escaped_value)
    }

    /// Generate build context arguments for docker buildx build command
    ///
    /// When `build_options` is provided and not default, cache arguments are included
    /// in the generated command. This enables cache-from/cache-to/no-cache/builder
    /// options to propagate to feature builds.
    ///
    /// Per spec (data-model.md):
    /// - `cache_from`: ordered list of cache sources, preserved when invoking BuildKit/buildx
    /// - `cache_to`: optional cache destination
    /// - `builder`: optional buildx builder selection
    /// - When `build_options.is_default()` returns true, no extra arguments are added
    ///
    /// No builder is pinned here. [`DockerfileConfig::base_image`] on this path is the
    /// registry reference the configuration named, so the generated `FROM` resolves under
    /// any buildx driver and the builder the user selected is the one that runs (#595).
    /// deacon used to build a daemon-local intermediate tag and `FROM` it, which only a
    /// docker-driver builder could see — and pinning that driver to rescue the chain took
    /// OCI export, local cache export and multi-platform output away from every build.
    ///
    /// `extra_labels` are `key=value` pairs written onto the produced image — the
    /// `devcontainer.metadata` a caller computed ahead of the build, when it wants
    /// one recorded.
    pub fn generate_build_args(
        &self,
        dockerfile_path: &Path,
        image_tag: &str,
        build_options: Option<&BuildOptions>,
        extra_labels: &[String],
    ) -> Vec<String> {
        let mut args = vec![
            "buildx".to_string(),
            "build".to_string(),
            "--load".to_string(),
        ];

        // Add cache/builder arguments from BuildOptions if provided and not default
        if let Some(opts) = build_options {
            if !opts.is_default() {
                args.extend(opts.to_docker_args());
            }
        }

        // Build-time host-CA build context (016): provides the bundle + install
        // script the generated RUN step mounts. Only present when injection is on.
        if let Some(ref ca_dir) = self.config.host_ca_build_context {
            args.push("--build-context".to_string());
            args.push(format!("{}={}", HOST_CA_BUILD_CONTEXT, ca_dir));
        }

        for label in extra_labels {
            args.push("--label".to_string());
            args.push(label.clone());
        }

        // Add build context and other standard arguments
        args.extend(vec![
            "--build-context".to_string(),
            format!(
                "{}={}",
                FEATURE_CONTENT_SOURCE, self.config.features_source_dir
            ),
            "--build-arg".to_string(),
            format!("_DEV_CONTAINERS_BASE_IMAGE={}", self.config.base_image),
            "--target".to_string(),
            self.config.target_stage.clone(),
            "-f".to_string(),
            dockerfile_path.display().to_string(),
            "-t".to_string(),
            image_tag.to_string(),
            ".".to_string(),
        ]);

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{FeatureMetadata, ResolvedFeature};

    fn create_test_feature(id: &str, options: HashMap<String, OptionValue>) -> ResolvedFeature {
        ResolvedFeature {
            id: id.to_string(),
            source: "ghcr.io/devcontainers/features".to_string(),
            options,
            metadata: FeatureMetadata {
                id: id.to_string(),
                name: Some(format!("Test {}", id)),
                version: Some("1.0.0".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                customizations: None,
                mounts: Vec::new(),
                entrypoint: None,
                privileged: None,
                init: None,
                cap_add: Vec::new(),
                security_opt: Vec::new(),
                depends_on: HashMap::new(),
                installs_after: Vec::new(),
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        }
    }

    fn create_test_feature_with_env(
        id: &str,
        options: HashMap<String, OptionValue>,
        container_env: HashMap<String, String>,
    ) -> ResolvedFeature {
        let mut feature = create_test_feature(id, options);
        feature.metadata.container_env = container_env;
        feature
    }

    /// A feature's `containerEnv` must be emitted as `ENV` lines AFTER its own
    /// install `RUN` and BEFORE any subsequent feature's `RUN`, so later
    /// features (and the final image) inherit it. Regression test for the
    /// `npm: command not found` failure when `ai-clis` installs after the
    /// `node` feature, whose nvm PATH lives only in `containerEnv`.
    #[test]
    fn test_container_env_emitted_as_env_between_features() {
        let mut node_env = HashMap::new();
        node_env.insert(
            "PATH".to_string(),
            "/usr/local/share/nvm/current/bin:${PATH}".to_string(),
        );
        node_env.insert("NVM_DIR".to_string(), "/usr/local/share/nvm".to_string());

        let node = create_test_feature_with_env("node", HashMap::new(), node_env);
        // A later feature with no containerEnv of its own.
        let ai = create_test_feature_with_env("ai-clis", HashMap::new(), HashMap::new());
        let plan = InstallationPlan::new(vec![node, ai]);

        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };
        let dockerfile = DockerfileGenerator::new(config).generate(&plan).unwrap();

        // The nvm PATH is emitted verbatim (Docker expands `${PATH}` itself).
        let env_line = "ENV PATH=\"/usr/local/share/nvm/current/bin:${PATH}\"";
        assert!(
            dockerfile.contains(env_line),
            "expected node containerEnv PATH as an ENV line:\n{dockerfile}"
        );
        assert!(dockerfile.contains("ENV NVM_DIR=\"/usr/local/share/nvm\""));

        // Ordering: node's install RUN < node's ENV < ai-clis's install RUN.
        // The mount `source=` suffix is the INSTALL-ORDER index, so two features in
        // the same parallel level get `_0` and `_1` rather than colliding on `_0`.
        let node_run = dockerfile.find("source=node_0").unwrap();
        let env_pos = dockerfile.find(env_line).unwrap();
        let ai_run = dockerfile.find("source=ai-clis_1").unwrap();
        assert!(
            node_run < env_pos && env_pos < ai_run,
            "containerEnv ENV must sit after node's RUN and before ai-clis's RUN"
        );

        // A feature with an empty containerEnv emits no stray ENV line for it.
        // (Only the two node ENV lines exist in the whole file.)
        assert_eq!(dockerfile.matches("\nENV ").count(), 2);
    }

    /// #641 — the CONFIGURATION's `containerEnv` is emitted after every Feature's,
    /// which is what makes it win where both declare a name. Asserted on BOTH
    /// entry points, because the reference has one template for all build shapes
    /// and covering only the one `build` happens to use would leave `up`'s
    /// `image:` shape and the Compose path silently divergent.
    #[test]
    fn config_container_env_is_emitted_after_every_feature_env() {
        let mut feature_env = HashMap::new();
        feature_env.insert("FEATURE_VAR".to_string(), "from-feature".to_string());
        feature_env.insert("OVERRIDDEN_VAR".to_string(), "from-feature".to_string());
        let plan = InstallationPlan::new(vec![create_test_feature_with_env(
            "localfeat",
            HashMap::new(),
            feature_env,
        )]);

        let mut config_env = IndexMap::new();
        config_env.insert("OVERRIDDEN_VAR".to_string(), "from-config".to_string());
        config_env.insert("CONFIG_VAR".to_string(), "from-config".to_string());

        let config = DockerfileConfig {
            base_image: "debian:bookworm-slim".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            config_container_env: config_env,
            ..Default::default()
        };
        let generator = DockerfileGenerator::new(config);

        for (which, dockerfile) in [
            ("generate", generator.generate(&plan).unwrap()),
            (
                "generate_install_stage_from",
                generator
                    .generate_install_stage_from(&plan, "base_stage")
                    .unwrap(),
            ),
        ] {
            let feature_wins = dockerfile
                .find("ENV OVERRIDDEN_VAR=\"from-feature\"")
                .unwrap_or_else(|| panic!("{which}: feature ENV missing:\n{dockerfile}"));
            let config_wins = dockerfile
                .find("ENV OVERRIDDEN_VAR=\"from-config\"")
                .unwrap_or_else(|| panic!("{which}: config ENV missing:\n{dockerfile}"));
            assert!(
                feature_wins < config_wins,
                "{which}: the config's ENV must come LAST, or the Feature's value survives"
            );
            assert!(
                dockerfile.contains("ENV CONFIG_VAR=\"from-config\""),
                "{which}: a config-only variable must reach the image:\n{dockerfile}"
            );
            // Authored order, not sorted — `OVERRIDDEN_VAR` was written first.
            assert!(
                config_wins < dockerfile.find("ENV CONFIG_VAR=").unwrap(),
                "{which}: the config's authored order must be preserved"
            );
        }
    }

    /// #641 — a configuration's value is meant LITERALLY, so `$` is escaped along
    /// with `"` and `\`. Without the `$` escape, `value with $dollar sign` reaches
    /// the image as `value with ` — Docker expands the unset `$dollar` to nothing.
    /// The Feature half deliberately does NOT escape `$` (a Feature's
    /// `${PATH}` self-reference must still expand), so the two forms are asserted
    /// against each other rather than in isolation.
    #[test]
    fn config_container_env_values_reach_the_image_literally() {
        let awkward = [
            ("VAR_WITH_SPACES", "value with spaces", "value with spaces"),
            (
                "VAR_WITH_LOTS_OF_SPACES",
                "    value with lots of spaces.   ",
                "    value with lots of spaces.   ",
            ),
            (
                "VAR_WITH_QUOTES",
                "value with \"quotes\" we want to keep",
                "value with \\\"quotes\\\" we want to keep",
            ),
            (
                "VAR_WITH_DOLLAR_SIGN",
                "value with $dollar sign",
                "value with \\$dollar sign",
            ),
            (
                "VAR_WITH_BACK_SLASH",
                "value with \\back slash",
                "value with \\\\back slash",
            ),
            (
                "ENV_WITH_COMMAND",
                "bash -c 'echo -n \"Hello, World!\"'",
                "bash -c 'echo -n \\\"Hello, World!\\\"'",
            ),
        ];

        for (key, value, expected) in awkward {
            assert_eq!(
                DockerfileGenerator::format_env_var_literal(key, value),
                format!("{key}=\"{expected}\""),
            );
        }

        // The Feature form differs on exactly one character class.
        assert_eq!(
            DockerfileGenerator::format_env_var("PATH", "/nvm/bin:${PATH}"),
            "PATH=\"/nvm/bin:${PATH}\"",
            "a Feature's self-reference must stay expandable"
        );
    }

    /// #641 — a configuration that declares no `containerEnv` must produce the
    /// Dockerfile it produced before this existed. Both CLIs bake nothing in that
    /// case and that agreement is what must not break.
    #[test]
    fn no_config_container_env_emits_no_trailing_env_lines() {
        let plan = InstallationPlan::new(vec![create_test_feature("go", HashMap::new())]);
        let config = DockerfileConfig {
            base_image: "debian:bookworm-slim".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };
        let generator = DockerfileGenerator::new(config);
        assert_eq!(
            generator.generate(&plan).unwrap().matches("\nENV ").count(),
            0
        );
        assert_eq!(
            generator
                .generate_install_stage_from(&plan, "base_stage")
                .unwrap()
                .matches("\nENV ")
                .count(),
            0
        );
    }

    /// The staged directory name is keyed on the feature's OWN id and its position in
    /// the install order — the two properties `read-configuration` needs to report a
    /// `cachePath` that names the directory a build would really create.
    #[test]
    fn feature_staging_dir_name_uses_the_metadata_id_and_install_index() {
        // The canonical id differs per command (registry path in `up`, short id in
        // `read-configuration`); the metadata id is what both agree on.
        let mut feature = create_test_feature("go", HashMap::new());
        feature.id = "ghcr.io/devcontainers/features/go".to_string();
        assert_eq!(
            DockerfileGenerator::feature_staging_dir_name(&feature, 0),
            "go_0"
        );
        assert_eq!(
            DockerfileGenerator::feature_staging_dir_name(&feature, 3),
            "go_3"
        );

        // A local feature's canonical id is `local:<absolute path>`; the staged name
        // must not carry a host path.
        let mut local = create_test_feature("needs-node", HashMap::new());
        local.id = "local:/ws/.devcontainer/needs-node".to_string();
        assert_eq!(
            DockerfileGenerator::feature_staging_dir_name(&local, 1),
            "needs-node_1"
        );

        // No metadata id: fall back to the canonical id rather than emit a bare `_2`.
        let mut anonymous = create_test_feature("x", HashMap::new());
        anonymous.metadata.id = String::new();
        anonymous.id = "ghcr.io/o/n/x".to_string();
        assert_eq!(
            DockerfileGenerator::feature_staging_dir_name(&anonymous, 2),
            "ghcr_io_o_n_x_2"
        );
    }

    #[test]
    fn test_sanitize_feature_id() {
        assert_eq!(
            DockerfileGenerator::sanitize_feature_id("ghcr.io/devcontainers/features/node:1"),
            "ghcr_io_devcontainers_features_node_1"
        );
        assert_eq!(
            DockerfileGenerator::sanitize_feature_id("common-utils"),
            "common-utils"
        );
    }

    #[test]
    fn test_sanitize_option_key() {
        // Spec parity (#88): non-identifier characters in feature option
        // ids must be replaced with `_`, and the whole key uppercased,
        // before being passed to `install.sh` as a shell env var.
        assert_eq!(
            DockerfileGenerator::sanitize_option_key("my-string-option"),
            "MY_STRING_OPTION"
        );
        assert_eq!(
            DockerfileGenerator::sanitize_option_key("another.weird-key"),
            "ANOTHER_WEIRD_KEY"
        );
        assert_eq!(
            DockerfileGenerator::sanitize_option_key("flagOption"),
            "FLAGOPTION"
        );
        // Already-identifier keys are unaffected (except for uppercasing).
        assert_eq!(
            DockerfileGenerator::sanitize_option_key("version"),
            "VERSION"
        );
        // Underscores survive untouched.
        assert_eq!(
            DockerfileGenerator::sanitize_option_key("install_zsh"),
            "INSTALL_ZSH"
        );
    }

    #[test]
    fn test_option_value_to_string() {
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::Boolean(true)),
            "true"
        );
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::String("test".to_string())),
            "test"
        );
        assert_eq!(
            DockerfileGenerator::option_value_to_string(&OptionValue::Number(
                serde_json::Number::from(42)
            )),
            "42"
        );
    }

    #[test]
    fn test_format_env_var() {
        assert_eq!(
            DockerfileGenerator::format_env_var("VERSION", "1.0"),
            "VERSION=\"1.0\""
        );
        assert_eq!(
            DockerfileGenerator::format_env_var("PATH", "/usr/bin:/bin"),
            "PATH=\"/usr/bin:/bin\""
        );
        // Test escaping
        assert_eq!(
            DockerfileGenerator::format_env_var("VAR", "value with \"quotes\""),
            "VAR=\"value with \\\"quotes\\\"\""
        );
    }

    #[test]
    fn test_generate_simple_dockerfile() {
        let mut options = HashMap::new();
        options.insert("version".to_string(), OptionValue::String("20".to_string()));

        let feature = create_test_feature("node", options);
        let plan = InstallationPlan::new(vec![feature]);

        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };

        let generator = DockerfileGenerator::new(config);
        let dockerfile = generator.generate(&plan).unwrap();

        assert!(dockerfile.contains("ARG _DEV_CONTAINERS_BASE_IMAGE=ubuntu:22.04"));
        assert!(
            dockerfile
                .contains("FROM ${_DEV_CONTAINERS_BASE_IMAGE} AS dev_containers_target_stage")
        );
        assert!(dockerfile.contains("RUN mkdir -p /tmp/dev-container-features"));
        assert!(dockerfile.contains("RUN --mount=type=bind"));
        assert!(dockerfile.contains("VERSION=\"20\""));
        assert!(dockerfile.contains("./install.sh"));
    }

    /// Spec parity (#89): `FeatureInstallEnv::resolve` implements the
    /// `_REMOTE_USER` → `_CONTAINER_USER` → image `USER` fallback chain.
    #[test]
    fn test_feature_install_env_resolution_rules() {
        // remoteUser defaults to containerUser.
        let env = FeatureInstallEnv::resolve(None, Some("builder"), Some("root"));
        assert_eq!(env.remote_user.as_deref(), Some("builder"));
        assert_eq!(env.container_user.as_deref(), Some("builder"));

        // containerUser defaults to the image's USER.
        let env = FeatureInstallEnv::resolve(None, None, Some("node"));
        assert_eq!(env.remote_user.as_deref(), Some("node"));
        assert_eq!(env.container_user.as_deref(), Some("node"));

        // An explicit remoteUser does not leak into containerUser.
        let env = FeatureInstallEnv::resolve(Some("vscode"), None, Some("root"));
        assert_eq!(env.remote_user.as_deref(), Some("vscode"));
        assert_eq!(env.container_user.as_deref(), Some("root"));

        // Home dirs: root is /root, everyone else /home/<user>.
        assert_eq!(env.remote_user_home.as_deref(), Some("/home/vscode"));
        assert_eq!(env.container_user_home.as_deref(), Some("/root"));

        // Nothing known: stays None so the caller can surface the gap.
        let env = FeatureInstallEnv::resolve(None, None, None);
        assert_eq!(env.remote_user, None);
        assert_eq!(env.container_user_home, None);
    }

    /// Spec parity (#89): the resolved users must actually reach the generated
    /// `install.sh` invocation. Features commonly do `su - "$_REMOTE_USER" -c`,
    /// which fails outright on an empty value — so assert the exported values,
    /// not merely that the variable names appear.
    #[test]
    fn test_feature_install_env_is_exported_into_install_command() {
        let feature = create_test_feature("ai-clis", HashMap::new());
        let plan = InstallationPlan::new(vec![feature]);

        let config = DockerfileConfig {
            base_image: "mcr.microsoft.com/devcontainers/base:ubuntu-24.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            feature_install_env: FeatureInstallEnv::resolve(Some("vscode"), None, Some("root")),
            ..Default::default()
        };

        let dockerfile = DockerfileGenerator::new(config).generate(&plan).unwrap();

        assert!(
            dockerfile.contains(r#"export _REMOTE_USER="vscode""#),
            "resolved _REMOTE_USER must be exported, got:\n{}",
            dockerfile
        );
        assert!(dockerfile.contains(r#"export _REMOTE_USER_HOME="/home/vscode""#));
        assert!(dockerfile.contains(r#"export _CONTAINER_USER="root""#));
        assert!(dockerfile.contains(r#"export _CONTAINER_USER_HOME="/root""#));
        assert!(
            !dockerfile.contains(r#"export _REMOTE_USER="""#),
            "an empty _REMOTE_USER breaks `su - \"$_REMOTE_USER\"` in feature install scripts"
        );
    }

    /// Unresolvable users still emit the variables (as empty strings) so that
    /// `${_REMOTE_USER:-}` resolves to "" rather than `<unset>`.
    #[test]
    fn test_unresolved_feature_install_env_still_exports_empty_values() {
        let feature = create_test_feature("node", HashMap::new());
        let plan = InstallationPlan::new(vec![feature]);

        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            feature_install_env: FeatureInstallEnv::resolve(None, None, None),
            ..Default::default()
        };

        let dockerfile = DockerfileGenerator::new(config).generate(&plan).unwrap();
        assert!(dockerfile.contains(r#"export _REMOTE_USER="""#));
        assert!(dockerfile.contains(r#"export _CONTAINER_USER="""#));
    }

    /// 016 / T034: the host-CA install RUN step is emitted after the features
    /// mkdir and BEFORE the first feature RUN-mount, only when injection is on,
    /// and is byte-stable for the same CA set (FR-017).
    #[test]
    fn test_host_ca_run_step_ordering_and_byte_stability() {
        let mut options = HashMap::new();
        options.insert("version".to_string(), OptionValue::String("20".to_string()));
        let feature = create_test_feature("node", options);
        let plan = InstallationPlan::new(vec![feature]);

        // Without injection: no CA step (default output unchanged).
        let off = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };
        let off_df = DockerfileGenerator::new(off).generate(&plan).unwrap();
        assert!(!off_df.contains("deacon_ca_source"));

        // With injection: CA step present, ordered correctly.
        let on = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            host_ca_build_context: Some("/tmp/deacon-ca-ctx".to_string()),
            ..Default::default()
        };
        let df1 = DockerfileGenerator::new(on.clone())
            .generate(&plan)
            .unwrap();
        let mkdir_pos = df1
            .find("RUN mkdir -p /tmp/dev-container-features")
            .unwrap();
        let ca_pos = df1
            .find("--mount=type=bind,from=deacon_ca_source")
            .expect("CA step present");
        let first_feature_pos = df1
            .find("--mount=type=bind,from=dev_containers_feature_content_source")
            .unwrap();
        assert!(
            mkdir_pos < ca_pos && ca_pos < first_feature_pos,
            "CA step must sit between mkdir and the first feature mount"
        );
        assert!(df1.contains("sh /tmp/deacon-ca/install.sh"));

        // Byte-stable: same CA set + image → identical Dockerfile text.
        let df2 = DockerfileGenerator::new(on).generate(&plan).unwrap();
        assert_eq!(df1, df2);

        // The build args carry the CA build context.
        let args = DockerfileGenerator::new(DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            host_ca_build_context: Some("/tmp/deacon-ca-ctx".to_string()),
            ..Default::default()
        })
        .generate_build_args(
            std::path::Path::new("/tmp/Dockerfile"),
            "img:tag",
            None,
            &[],
        );
        assert!(
            args.iter()
                .any(|a| a == "deacon_ca_source=/tmp/deacon-ca-ctx")
        );
    }

    /// Bead 14b: when extending a user-authored Dockerfile (compose `build:`
    /// shape) the install stage must use a literal `FROM <stage>` — never the
    /// ARG-driven `FROM ${...}` form — because global-ARG substitution in
    /// FROM only works when the ARG is declared before any FROM, which is
    /// impossible when we append after user stages.
    #[test]
    fn test_generate_install_stage_from_uses_literal_from_stage() {
        let mut options = HashMap::new();
        options.insert(
            "version".to_string(),
            OptionValue::String("latest".to_string()),
        );
        let feature = create_test_feature("hello", options);
        let plan = InstallationPlan::new(vec![feature]);

        let config = DockerfileConfig {
            base_image: "unused-for-this-path".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };
        let generator = DockerfileGenerator::new(config);
        let stage = generator
            .generate_install_stage_from(&plan, "user_image")
            .expect("generation should succeed");

        // Literal FROM with the user's stage name, no ARG indirection.
        assert!(
            stage.contains("FROM user_image AS dev_containers_target_stage\n"),
            "expected literal FROM line; got:\n{}",
            stage
        );
        assert!(
            !stage.contains("ARG _DEV_CONTAINERS_BASE_IMAGE"),
            "install-stage variant must NOT emit the ARG indirection; got:\n{}",
            stage
        );
        assert!(!stage.contains("${_DEV_CONTAINERS_BASE_IMAGE}"));

        // The RUN-mount install line is still emitted.
        assert!(stage.contains("RUN --mount=type=bind"));
        assert!(stage.contains("./install.sh"));
        assert!(stage.contains("VERSION=\"latest\""));
    }

    #[test]
    fn test_generate_build_args() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            None,
            &[],
        );

        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));
        assert!(args.contains(&"--load".to_string()));
        assert!(args.contains(&"--build-context".to_string()));
        assert!(args.contains(&"dev_containers_feature_content_source=/tmp/features".to_string()));
        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"test:latest".to_string()));
        // No cache arguments when build_options is None
        assert!(!args.contains(&"--cache-from".to_string()));
        assert!(!args.contains(&"--cache-to".to_string()));
    }

    #[test]
    fn test_generate_build_args_with_cache_options() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };

        let build_options = BuildOptions {
            no_cache: false,
            cache_from: vec![
                "type=registry,ref=myrepo/cache:v1".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: Some("type=registry,ref=myrepo/cache:latest".to_string()),
            builder: Some("mybuilder".to_string()),
            platform: None,
            output_mode: crate::build::BuildOutputMode::default(),
        };

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            Some(&build_options),
            &[],
        );

        // Standard args still present
        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));
        assert!(args.contains(&"--load".to_string()));

        // Cache args from BuildOptions
        assert!(args.contains(&"--cache-from".to_string()));
        assert!(args.contains(&"type=registry,ref=myrepo/cache:v1".to_string()));
        assert!(args.contains(&"type=local,src=/tmp/cache".to_string()));
        assert!(args.contains(&"--cache-to".to_string()));
        assert!(args.contains(&"type=registry,ref=myrepo/cache:latest".to_string()));
        assert!(args.contains(&"--builder".to_string()));
        assert!(args.contains(&"mybuilder".to_string()));
    }

    #[test]
    fn test_generate_build_args_with_default_options() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };

        // Default options should not add any cache arguments
        let build_options = BuildOptions::default();
        assert!(build_options.is_default());

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            Some(&build_options),
            &[],
        );

        // Standard args present
        assert!(args.contains(&"buildx".to_string()));
        assert!(args.contains(&"build".to_string()));

        // No cache arguments when build_options is default
        assert!(!args.contains(&"--cache-from".to_string()));
        assert!(!args.contains(&"--cache-to".to_string()));
        assert!(!args.contains(&"--no-cache".to_string()));

        // And no builder is named either. deacon used to pin `--builder default`
        // here because this build's `FROM` could name a daemon-local intermediate
        // tag; it no longer can, and pinning silently overrode the builder the
        // user selected, taking OCI export, cache export and multi-platform
        // output with it (#595).
        assert!(
            !args.contains(&"--builder".to_string()),
            "no builder may be pinned when none was requested: {args:?}"
        );
    }

    /// The only `--builder` that ever appears is one the caller asked for.
    ///
    /// This also keeps Podman honest: `--builder` is a Docker CLI flag that Podman's build
    /// path rejects rather than ignores, and nothing here emits one unprompted.
    #[test]
    fn test_generate_build_args_explicit_builder_wins() {
        let config = DockerfileConfig {
            base_image: "ubuntu:22.04".to_string(),
            target_stage: "dev_containers_target_stage".to_string(),
            features_source_dir: "/tmp/features".to_string(),
            ..Default::default()
        };

        let build_options = BuildOptions {
            builder: Some("my-remote-builder".to_string()),
            ..Default::default()
        };

        let generator = DockerfileGenerator::new(config);
        let args = generator.generate_build_args(
            Path::new("/tmp/Dockerfile.extended"),
            "test:latest",
            Some(&build_options),
            &[],
        );

        let builders: Vec<&str> = args
            .windows(2)
            .filter(|w| w[0] == "--builder")
            .map(|w| w[1].as_str())
            .collect();
        assert_eq!(
            builders,
            vec!["my-remote-builder"],
            "an explicit builder must be the only one requested: {args:?}"
        );
    }
}
