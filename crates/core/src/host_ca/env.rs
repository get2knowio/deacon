//! CA-related environment variable names and the synthesized-env merge.
//!
//! When host-CA injection is enabled, deacon points the common TLS toolchains
//! at the in-container bundle by setting the six well-known CA env vars
//! ([`CA_ENV_VARS`]). Per Constitution V, every observable env-var name is a
//! named constant (no scattered string literals).

use std::collections::HashMap;

/// Env var the machine owner sets to activate injection: `auto` or a PEM path.
/// Resolved by the activation precedence helper (CLI flag > this > settings).
pub const DEACON_INJECT_HOST_CA: &str = "DEACON_INJECT_HOST_CA";

/// Canonical in-container path the corporate PEM bundle is always written to
/// (even on the env-var-only fallback), and the value the six CA env vars point
/// at. Fixed so `exec`/`run-user-commands` can re-apply the env from a label
/// without re-discovery.
pub const HOST_CA_BUNDLE_PATH: &str = "/usr/local/share/deacon/host-ca.crt";

/// The six CA-bundle environment variables deacon synthesizes, in a stable
/// order. Each toolchain reads a different one; all point at
/// [`HOST_CA_BUNDLE_PATH`]. User-provided values always win (insert-if-absent).
pub const CA_ENV_VARS: [&str; 6] = [
    "SSL_CERT_FILE",       // OpenSSL / many libs
    "NODE_EXTRA_CA_CERTS", // Node.js
    "REQUESTS_CA_BUNDLE",  // Python requests
    "PIP_CERT",            // pip
    "GIT_SSL_CAINFO",      // git over https
    "CURL_CA_BUNDLE",      // curl
];

/// Insert the six CA env vars into `env`, pointing each at `bundle_path`, but
/// only when the user has not already set that variable.
///
/// Mirrors the secrets `or_insert_with` merge so user `containerEnv`/`remoteEnv`
/// + CLI `--remote-env` values win over the synthesized defaults (FR-024).
pub fn apply_ca_env_vars(env: &mut HashMap<String, String>, bundle_path: &str) {
    for name in CA_ENV_VARS {
        env.entry(name.to_string())
            .or_insert_with(|| bundle_path.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_all_six_when_absent() {
        let mut env = HashMap::new();
        apply_ca_env_vars(&mut env, HOST_CA_BUNDLE_PATH);
        assert_eq!(env.len(), 6);
        for name in CA_ENV_VARS {
            assert_eq!(env.get(name).map(String::as_str), Some(HOST_CA_BUNDLE_PATH));
        }
    }

    #[test]
    fn user_value_wins() {
        let mut env = HashMap::new();
        env.insert("SSL_CERT_FILE".to_string(), "/user/custom.pem".to_string());
        apply_ca_env_vars(&mut env, HOST_CA_BUNDLE_PATH);
        // User's SSL_CERT_FILE preserved; the other five synthesized.
        assert_eq!(
            env.get("SSL_CERT_FILE").map(String::as_str),
            Some("/user/custom.pem")
        );
        assert_eq!(
            env.get("CURL_CA_BUNDLE").map(String::as_str),
            Some(HOST_CA_BUNDLE_PATH)
        );
        assert_eq!(env.len(), 6);
    }

    #[test]
    fn idempotent_second_apply_is_noop() {
        let mut env = HashMap::new();
        apply_ca_env_vars(&mut env, HOST_CA_BUNDLE_PATH);
        apply_ca_env_vars(&mut env, "/other/path.pem");
        // First apply wins; second is a no-op (all already present).
        for name in CA_ENV_VARS {
            assert_eq!(env.get(name).map(String::as_str), Some(HOST_CA_BUNDLE_PATH));
        }
    }
}
