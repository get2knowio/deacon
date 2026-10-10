//! What OCI auth hardening WOULD do, observed while not enforcing it.
//!
//! The reference CLI emits an `ociAuthDiagnostics` object from
//! `read-configuration` (added at oracle 0.89.0), and it is easy to mistake for the
//! output of a feature deacon does not have. It is not. Upstream's
//! `recordOCIAuthDiagnostic` never consults `params.ociAuthHardening` — the flag
//! gates ENFORCEMENT at five separate sites, while these three booleans are recorded
//! unconditionally on the ordinary path. They answer a migration question: *if you
//! turned hardening on, what would have broken?*
//!
//! So this is an observation, not a capability. Every input is something deacon
//! already handles — it performs the token request, it parses the `realm` out of the
//! `WWW-Authenticate` challenge, and it knows which registry the reference named.
//! What was missing was the small policy predicate, which is ported here.
//!
//! All three default to `false`, and `false` here means "not observed", which is the
//! truth when no registry was contacted at all. That matters for parity: the
//! reference emits the object on every `read-configuration`, including one that
//! resolves no Features, so the absence of traffic is reported as three falses rather
//! than as a missing field.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tracing::info;

/// Hostnames Docker Hub answers distribution requests on.
///
/// Upstream keeps the same set (`dockerHubRegistryHosts`) because a Docker Hub
/// reference and the request it produces legitimately use different authorities — a
/// challenge arriving from `auth.docker.io` for a `registry-1.docker.io` request is
/// the registry talking to itself, not a redirect somewhere else. Comparing origins
/// without this equivalence would record a diagnostic on every Docker Hub pull.
const DOCKER_HUB_REGISTRY_HOSTS: &[&str] = &[
    "docker.io",
    "index.docker.io",
    "registry-1.docker.io",
    "registry.hub.docker.com",
];

/// The three observations, as the reference serializes them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OciAuthDiagnosticsSnapshot {
    /// A token-service realm that hardening would refuse to contact: not
    /// same-authority with the registry, and not a configured cross-origin host.
    pub auth_lookup_would_be_blocked: bool,
    /// A request or challenge origin that hardening would refuse to forward the
    /// registry's credentials to.
    pub registry_redirect_would_prevent_credential_forwarding: bool,
    /// The token request was redirected.
    pub auth_server_redirect: bool,
}

/// Accumulator shared with whatever is making registry requests.
///
/// Atomics rather than a mutex: three independent set-once flags need no
/// transactional view, and `HttpClient` implementations are `Send + Sync` and used
/// concurrently. `Relaxed` is sufficient because nothing orders on these — the
/// snapshot is read after the work completes, and each flag's only transition is
/// `false -> true`.
#[derive(Debug, Default)]
pub struct OciAuthDiagnostics {
    auth_lookup_would_be_blocked: AtomicBool,
    registry_redirect_would_prevent_credential_forwarding: AtomicBool,
    auth_server_redirect: AtomicBool,
}

impl OciAuthDiagnostics {
    /// A fresh accumulator, shareable by clone.
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record once, and log the reason the first time — mirroring upstream, which
    /// writes its explanatory message only on the transition so a repeated
    /// observation does not repeat the line.
    fn record(flag: &AtomicBool, message: &str) {
        if !flag.swap(true, Ordering::Relaxed) {
            info!("OCI auth diagnostics: {}", message);
        }
    }

    /// The token request for `requested_url` ended at `final_url`.
    ///
    /// Upstream sets the flag whenever the response was redirected, whether or not
    /// the origin changed, and distinguishes the two only in the message. reqwest
    /// exposes no `redirected` boolean, so the faithful proxy is that the final URL
    /// differs from the requested one.
    pub fn observe_token_request(&self, requested_url: &str, final_url: &str) {
        if requested_url == final_url {
            return;
        }
        let requested_origin = origin_of(requested_url);
        let final_origin = origin_of(final_url);
        let where_to = match (&requested_origin, &final_origin) {
            (Some(a), Some(b)) if a == b => format!("within origin '{a}'"),
            (Some(a), Some(b)) => format!("from origin '{a}' to '{b}'"),
            _ => format!("from '{requested_url}' to '{final_url}'"),
        };
        Self::record(
            &self.auth_server_redirect,
            &format!("Authentication server redirected a token request {where_to}."),
        );
    }

    /// A `401`/`403` challenge arrived for `requested_url`, answered from
    /// `response_url`, while fetching from `registry`.
    ///
    /// Upstream records when EITHER the request was not to the registry's own origin
    /// or the challenge did not come from it — under hardening, the registry's
    /// credentials would be withheld in both cases.
    pub fn observe_auth_challenge(&self, registry: &str, requested_url: &str, response_url: &str) {
        let request_can_use_credentials = is_oci_registry_origin(requested_url, registry);
        let challenge_from_registry = is_oci_registry_origin(response_url, registry);
        if request_can_use_credentials && challenge_from_registry {
            return;
        }
        let requested_host = host_of(requested_url).unwrap_or_else(|| requested_url.to_string());
        let challenge_host = host_of(response_url).unwrap_or_else(|| response_url.to_string());
        Self::record(
            &self.registry_redirect_would_prevent_credential_forwarding,
            &format!(
                "Request to '{requested_host}' with authentication challenge from \
                 '{challenge_host}' would prevent forwarding registry '{registry}' \
                 credentials with OCI auth hardening."
            ),
        );
    }

    /// The challenge named `realm` as its token service, for `registry`.
    pub fn observe_token_service_realm(&self, registry: &str, realm: &str) {
        if is_allowed_token_service_realm(realm, registry) {
            return;
        }
        let realm_origin = origin_of(realm).unwrap_or_else(|| realm.to_string());
        let registry_host = host_of(registry)
            .or_else(|| Some(registry.to_string()))
            .unwrap_or_default();
        Self::record(
            &self.auth_lookup_would_be_blocked,
            &format!(
                "Authentication lookup from registry '{registry_host}' to realm origin \
                 '{realm_origin}' would be blocked by OCI auth hardening."
            ),
        );
    }

    /// Read the accumulated observations.
    pub fn snapshot(&self) -> OciAuthDiagnosticsSnapshot {
        OciAuthDiagnosticsSnapshot {
            auth_lookup_would_be_blocked: self.auth_lookup_would_be_blocked.load(Ordering::Relaxed),
            registry_redirect_would_prevent_credential_forwarding: self
                .registry_redirect_would_prevent_credential_forwarding
                .load(Ordering::Relaxed),
            auth_server_redirect: self.auth_server_redirect.load(Ordering::Relaxed),
        }
    }
}

/// `scheme://host[:port]` of `url`, lowercased, or `None` if it does not parse.
fn origin_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(match parsed.port() {
        Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
        None => format!("{}://{}", parsed.scheme(), host),
    })
}

/// Authority (`host[:port]`) of `url`, lowercased.
fn host_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// Scheme a registry authority is reached over: `http` only for `localhost`.
///
/// Upstream's `getRegistryScheme`, which takes the URL's HOSTNAME — so a port is
/// ignored and `localhost:5000` is `http`.
fn registry_scheme(registry: &str) -> &'static str {
    let host = registry
        .rsplit_once(':')
        .map_or(registry, |(host, _port)| host);
    if host.eq_ignore_ascii_case("localhost") {
        "http"
    } else {
        "https"
    }
}

/// Is `url` the registry's own origin — upstream's `isOCIRegistryOrigin`?
///
/// Exact origin match, or both sides HTTPS Docker Hub authorities.
fn is_oci_registry_origin(url: &str, registry: &str) -> bool {
    let Some(url_origin) = origin_of(url) else {
        return false;
    };
    let registry_url = format!("{}://{}", registry_scheme(registry), registry);
    let Some(registry_origin) = origin_of(&registry_url) else {
        return false;
    };
    if url_origin == registry_origin {
        return true;
    }
    let (Some(url_host), Some(registry_host)) = (host_of(url), host_of(&registry_url)) else {
        return false;
    };
    url_origin.starts_with("https://")
        && registry_origin.starts_with("https://")
        && DOCKER_HUB_REGISTRY_HOSTS.contains(&url_host.as_str())
        && DOCKER_HUB_REGISTRY_HOSTS.contains(&registry_host.as_str())
}

/// Would hardening permit contacting `realm` as `registry`'s token service?
///
/// Upstream's `isAllowedTokenServiceRealmForPolicy`: same authority, or HTTPS and a
/// CONFIGURED cross-origin host. deacon has no `--allow-cross-origin-auth-host`, so
/// its configured set is empty and the second arm can never hold. That is a correct
/// input rather than a stand-in: "with no configured exceptions, would this realm be
/// blocked?" has a true answer, and it is the answer deacon reports.
fn is_allowed_token_service_realm(realm: &str, registry: &str) -> bool {
    let registry_url = format!("{}://{}", registry_scheme(registry), registry);
    match (host_of(realm), host_of(&registry_url)) {
        (Some(realm_host), Some(registry_host)) => realm_host == registry_host,
        // An unparseable realm is not same-authority with anything, and with no
        // configured exceptions it would be blocked.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_accumulator_reports_nothing_observed() {
        // The reference emits the object on every `read-configuration`, including one
        // that contacts no registry, so "no traffic" must read as three falses rather
        // than as a missing field.
        let d = OciAuthDiagnostics::default();
        assert_eq!(d.snapshot(), OciAuthDiagnosticsSnapshot::default());
        assert!(!d.snapshot().auth_server_redirect);
    }

    #[test]
    fn a_redirected_token_request_is_recorded_and_an_unredirected_one_is_not() {
        let d = OciAuthDiagnostics::default();
        d.observe_token_request("https://ghcr.io/token?x=1", "https://ghcr.io/token?x=1");
        assert!(
            !d.snapshot().auth_server_redirect,
            "same URL is no redirect"
        );

        d.observe_token_request("https://ghcr.io/token", "https://auth.ghcr.io/token");
        assert!(d.snapshot().auth_server_redirect);
    }

    #[test]
    fn a_same_origin_redirect_still_counts() {
        // Upstream sets the flag on `response.redirected` regardless of origin and
        // uses the origin only to word the message. A rule that required an origin
        // CHANGE would miss a within-origin redirect the reference reports.
        let d = OciAuthDiagnostics::default();
        d.observe_token_request("https://ghcr.io/token", "https://ghcr.io/token/v2");
        assert!(d.snapshot().auth_server_redirect);
    }

    #[test]
    fn a_challenge_from_the_registry_itself_is_not_a_credential_forwarding_risk() {
        let d = OciAuthDiagnostics::default();
        d.observe_auth_challenge(
            "ghcr.io",
            "https://ghcr.io/v2/x/manifests/1",
            "https://ghcr.io/v2/x/manifests/1",
        );
        assert!(
            !d.snapshot()
                .registry_redirect_would_prevent_credential_forwarding,
            "the ordinary case must not record a diagnostic"
        );
    }

    #[test]
    fn a_challenge_from_elsewhere_is_recorded() {
        let d = OciAuthDiagnostics::default();
        d.observe_auth_challenge(
            "ghcr.io",
            "https://ghcr.io/v2/x/manifests/1",
            "https://cdn.example.com/v2/x/manifests/1",
        );
        assert!(
            d.snapshot()
                .registry_redirect_would_prevent_credential_forwarding
        );
    }

    #[test]
    fn docker_hubs_several_authorities_are_the_same_registry() {
        // Without this equivalence a diagnostic would be recorded on every Docker Hub
        // pull, which is the noise upstream's `dockerHubRegistryHosts` exists to avoid.
        assert!(is_oci_registry_origin(
            "https://registry-1.docker.io/v2/library/alpine/manifests/3.19",
            "docker.io"
        ));
        assert!(is_oci_registry_origin(
            "https://index.docker.io/v2/",
            "registry.hub.docker.com"
        ));
        assert!(
            !is_oci_registry_origin("https://ghcr.io/v2/", "docker.io"),
            "the equivalence must not reach beyond Docker Hub"
        );
    }

    #[test]
    fn a_same_authority_realm_is_allowed_and_a_cross_origin_one_is_not() {
        assert!(is_allowed_token_service_realm(
            "https://ghcr.io/token",
            "ghcr.io"
        ));
        // deacon has no `--allow-cross-origin-auth-host`, so no cross-origin realm is
        // configured and every one of them would be blocked.
        assert!(!is_allowed_token_service_realm(
            "https://auth.ghcr.io/token",
            "ghcr.io"
        ));
        assert!(!is_allowed_token_service_realm("not a url", "ghcr.io"));
    }

    #[test]
    fn a_localhost_registry_is_matched_over_http_including_with_a_port() {
        // `registry_scheme` takes the HOSTNAME, so the port does not defeat it — the
        // shape every local-registry test uses.
        assert_eq!(registry_scheme("localhost:5000"), "http");
        assert_eq!(registry_scheme("LOCALHOST"), "http");
        assert_eq!(registry_scheme("ghcr.io"), "https");
        assert!(is_oci_registry_origin(
            "http://localhost:5000/v2/x/manifests/1",
            "localhost:5000"
        ));
        assert!(is_allowed_token_service_realm(
            "http://localhost:5000/token",
            "localhost:5000"
        ));
    }

    #[test]
    fn recording_twice_stays_recorded() {
        let d = OciAuthDiagnostics::default();
        d.observe_token_request("https://a.test/t", "https://b.test/t");
        d.observe_token_request("https://a.test/t", "https://b.test/t");
        assert!(d.snapshot().auth_server_redirect);
    }
}
