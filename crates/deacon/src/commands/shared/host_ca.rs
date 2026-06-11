//! Reconnect-path host-CA env re-application (016, T032/T033).
//!
//! `up` discovers + injects the corporate CA and records the in-container bundle
//! path on a container label (`devcontainer.deacon.hostCaBundlePath`). When
//! `exec` / `run-user-commands` reconnect to that container, they re-apply the
//! six CA env vars from the **label** — never re-running discovery or
//! re-resolving activation. User-provided env always wins (insert-if-absent).

use deacon_core::container::LABEL_HOST_CA_BUNDLE_PATH;
use deacon_core::docker::Docker;
use tracing::debug;

/// Read the in-container host-CA bundle path that `up` stamped on the container,
/// or `None` when injection was not enabled for this container.
pub async fn read_host_ca_bundle_path<D: Docker>(docker: &D, container_id: &str) -> Option<String> {
    match docker.inspect_container(container_id).await {
        Ok(Some(info)) => info.labels.get(LABEL_HOST_CA_BUNDLE_PATH).cloned(),
        Ok(None) => None,
        Err(e) => {
            debug!(
                "Could not inspect container {} for host-CA label: {}",
                container_id, e
            );
            None
        }
    }
}

/// Insert the six CA env vars into an `IndexMap` env (exec's CLI remote-env),
/// pointing each at the label-recorded `bundle_path`, insert-if-absent.
pub fn apply_ca_env_indexmap(env: &mut deacon_core::IndexMap<String, String>, bundle_path: &str) {
    for name in deacon_core::host_ca::CA_ENV_VARS {
        env.entry(name.to_string())
            .or_insert_with(|| bundle_path.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_ca_env_indexmap_inserts_and_respects_user() {
        let mut env = deacon_core::IndexMap::new();
        env.insert("SSL_CERT_FILE".to_string(), "/user.pem".to_string());
        apply_ca_env_indexmap(&mut env, "/usr/local/share/deacon/host-ca.crt");
        assert_eq!(
            env.get("SSL_CERT_FILE").map(String::as_str),
            Some("/user.pem")
        );
        assert_eq!(
            env.get("CURL_CA_BUNDLE").map(String::as_str),
            Some("/usr/local/share/deacon/host-ca.crt")
        );
        assert_eq!(env.len(), 6);
    }
}
