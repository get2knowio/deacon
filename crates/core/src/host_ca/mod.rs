//! Corporate-CA (host trust store) support — the opt-in, machine-side
//! capability that injects the corporate root CA delta into dev containers so
//! builds and runtimes "just work" behind a TLS-intercepting proxy (feature
//! 016).
//!
//! This module is deacon-specific (not mandated by containers.dev), gated to
//! machine-owner sources only (CLI flag / env / `settings.json`, never the
//! workspace — FR-015). See `SECURITY.md` for the threat model.
//!
//! Submodules:
//! - [`activation`] — resolve the [`HostCaActivation`] decision (precedence).
//! - [`discover`] — enumerate host roots + compute the corporate delta.
//! - [`inject`] — the in-container install script + runtime orchestration.
//! - [`env`] — CA env-var names + the synthesized-env merge.

pub mod activation;
pub mod discover;
pub mod env;
pub mod inject;

pub use activation::{HostCaActivation, resolve_host_ca_activation};
pub use discover::{
    CorporateCaSet, HostCertificate, discover_corporate_set, enumerate_host_roots,
    validate_explicit_bundle,
};
pub use env::{CA_ENV_VARS, DEACON_INJECT_HOST_CA, HOST_CA_BUNDLE_PATH, apply_ca_env_vars};
pub use inject::{
    InjectionMode, InjectionOutcome, build_install_script, inject_runtime, runtime_install_script,
};
