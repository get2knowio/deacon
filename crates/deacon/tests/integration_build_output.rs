//! Integration tests for build-output rendering (`deacon build`).
//!
//! These run through the streaming build executor (`run_build_once` / the
//! `BuildIo` path). In CI stderr is not a TTY, so the resolved mode is **Plain**:
//! build output is streamed verbatim to stderr. The key guarantees verified here:
//!
//! * a **failing** build surfaces the failing step's output on stderr (it is not
//!   swallowed) and exits non-zero, and
//! * a **successful** build still produces the expected JSON result on stdout.
//!
//! Both tolerate a Docker-less environment (they assert the Docker-unavailable
//! error instead), mirroring `integration_build.rs`.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Whether the failure is just "Docker isn't available here" (so the test's real
/// assertion doesn't apply).
fn is_docker_unavailable(stderr: &str) -> bool {
    let lc = stderr.to_lowercase();
    stderr.contains("Docker is not installed")
        || stderr.contains("Docker daemon is not")
        || lc.contains("permission denied")
        || lc.contains("cannot connect to the docker daemon")
}

fn write_devcontainer(temp_dir: &TempDir, dockerfile: &str) {
    fs::create_dir_all(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(temp_dir.path().join(".devcontainer/Dockerfile"), dockerfile).unwrap();
    let config = r#"{
    "name": "Build Output Test",
    "dockerFile": "Dockerfile",
    "build": { "context": "." }
}
"#;
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        config,
    )
    .unwrap();
}

/// A failing `RUN` must surface its output on stderr (Plain mode streams it, and
/// the build error carries the captured stderr) and the command must exit
/// non-zero — i.e. the failure is not silently swallowed.
#[test]
fn build_failure_surfaces_failing_step_output() {
    let temp_dir = TempDir::new().unwrap();
    // A unique marker printed by the failing RUN step. `--no-cache` guarantees the
    // step actually executes (not served from a prior layer cache).
    let marker = "DEACON_BUILD_OUTPUT_FAIL_MARKER";
    let dockerfile =
        format!("FROM alpine:3.19\nLABEL deacon.test=build-output\nRUN echo {marker} && exit 7\n");
    write_devcontainer(&temp_dir, &dockerfile);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--no-cache")
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    if is_docker_unavailable(&stderr) {
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    assert!(
        !output.status.success(),
        "a failing RUN must produce a non-zero exit; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains(marker),
        "the failing step's output must be surfaced on stderr, not swallowed; stderr:\n{stderr}"
    );
}

/// A successful build still emits the JSON result on stdout (stdout stays
/// reserved for the result; build progress goes to stderr).
#[test]
fn build_success_emits_json_result_on_stdout() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile = "FROM alpine:3.19\nLABEL deacon.test=build-output\nRUN echo DEACON_BUILD_OK\n";
    write_devcontainer(&temp_dir, dockerfile);

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        assert!(
            is_docker_unavailable(&stderr),
            "unexpected build failure (docker available): {stderr}"
        );
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    // stdout carries only the JSON result.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""outcome":"success"#),
        "stdout should carry the success JSON result; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains(r#""imageName""#),
        "stdout JSON should include the built image name; stdout:\n{stdout}"
    );
}

/// #470: two concurrent builds of byte-identical content must both succeed.
///
/// `calculate_config_hash` is content-derived and deliberately workspace-agnostic
/// (Dockerfile bytes plus each context file's relative path, size and
/// mtime-in-seconds), so two such builds name the SAME deterministic
/// `deacon-build:<hash>` tag. Each one's image nevertheless carries a distinct
/// digest — BuildKit emits a per-build attestation manifest — so whichever build
/// names the shared tag last leaves the other's image unreferenced, and the
/// containerd image store drops it. deacon used to resolve its own just-built
/// image through that raw digest, so the loser died with
/// `docker inspect: no such object` AFTER BuildKit had reported success.
///
/// The `--output` side is the one that breaks, because it passes no
/// `--image-name`: its sibling's user tag keeps the sibling's image referenced,
/// while the shared tag is the export side's ONLY reference. That is exactly the
/// parity case `case-build-output-export-tar`, which flaked twice on CI.
///
/// The fixtures' mtimes are pinned to one instant so the hash collision — and
/// hence the race — is guaranteed rather than merely likely.
#[test]
fn concurrent_identical_builds_do_not_race_on_the_shared_deterministic_tag() {
    use std::time::{Duration, SystemTime};

    let dockerfile = "FROM alpine:3.19\nLABEL deacon.test=build-output-470\n";
    // A fixed, shared mtime: the context-file component of the config hash is
    // (relative path, size, mtime-in-seconds), so pinning it makes both
    // workspaces hash identically no matter how far apart they are written.
    let pinned = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    let make_workspace = || {
        let dir = TempDir::new().unwrap();
        write_devcontainer(&dir, dockerfile);
        for name in [
            ".devcontainer/Dockerfile",
            ".devcontainer/devcontainer.json",
        ] {
            let f = fs::File::options()
                .write(true)
                .open(dir.path().join(name))
                .unwrap();
            f.set_modified(pinned).unwrap();
        }
        dir
    };

    let exporting = make_workspace();
    let plain = make_workspace();
    let tar_path = exporting.path().join("export.tar");
    let export_spec = format!("type=docker,dest={}", tar_path.display());
    let bin = assert_cmd::cargo::cargo_bin("deacon");

    // The victim shape: `--output`, no `--image-name`.
    let export_dir = exporting.path().to_path_buf();
    let export_bin = bin.clone();
    let exporter = std::thread::spawn(move || {
        let child = std::process::Command::new(export_bin)
            .current_dir(&export_dir)
            .args(["build", "--output", &export_spec, "--output-format", "json"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // The run-private tag is namespaced by the deacon process's pid, so
        // capturing it is what lets the leak check below scope itself to the
        // resources THIS test created rather than scanning daemon-global state.
        let pid = child.id();
        (pid, child.wait_with_output().unwrap())
    });

    // The sibling that re-points the shared tag out from under it.
    let plain_dir = plain.path().to_path_buf();
    let sibling = std::thread::spawn(move || {
        let child = std::process::Command::new(bin)
            .current_dir(&plain_dir)
            .args([
                "build",
                "--image-name",
                "deacon-test-470-sibling:latest",
                "--output-format",
                "json",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        (pid, child.wait_with_output().unwrap())
    });

    let (export_pid, export_out) = exporter.join().unwrap();
    let (sibling_pid, sibling_out) = sibling.join().unwrap();
    let export_err = String::from_utf8_lossy(&export_out.stderr);
    let sibling_err = String::from_utf8_lossy(&sibling_out.stderr);

    let _ = std::process::Command::new("docker")
        .args(["image", "rm", "deacon-test-470-sibling:latest"])
        .output();

    if !export_out.status.success() && is_docker_unavailable(&export_err) {
        eprintln!("skipping: docker unavailable ({})", export_err.trim());
        return;
    }

    assert!(
        sibling_out.status.success(),
        "the plain concurrent build should succeed; stderr:\n{sibling_err}"
    );
    assert!(
        export_out.status.success(),
        "the --output build lost the race for the shared deterministic tag (#470); stderr:\n{export_err}"
    );
    assert!(
        tar_path.exists(),
        "the --output build reported success but wrote no tar at {}",
        tar_path.display()
    );

    // The run-private tag is bookkeeping: it must not survive the build. Scoped
    // to the two pids this test spawned — a bare `deacon-build-run` substring
    // match would scan daemon-global state and report a sibling's in-flight tag
    // as this test's leak.
    let images = std::process::Command::new("docker")
        .args(["images", "--format", "{{.Repository}}:{{.Tag}}"])
        .output()
        .unwrap();
    let images = String::from_utf8_lossy(&images.stdout);
    for pid in [export_pid, sibling_pid] {
        let ours = format!("deacon-build-run:{pid}-");
        assert!(
            !images.contains(&ours),
            "the run-private build tag `{ours}*` leaked into the local image list"
        );
    }
}

/// #595: the builder the user selected is the one that runs, so an exporter only
/// a non-docker driver can serve is reachable.
///
/// deacon used to build the base to a daemon-local tag and then chain a Feature
/// build and a metadata-stamp build that `FROM`ed it. A `docker-container` driver
/// builder runs in an isolated BuildKit container that cannot read the daemon's
/// image store, so deacon pinned `--builder default` to make the chain work — and
/// that pin silently overrode the user's choice for EVERY build, taking OCI
/// export, local cache export and multi-platform output with it. The chain is
/// gone; the pin is gone with it.
///
/// `BUILDX_BUILDER` selects the builder for this invocation only; `docker buildx
/// use` would mutate host-global state that concurrent tests share. Worth knowing,
/// because the two are NOT equivalent to the Docker CLI: measured at CLI 29.7.2,
/// plain `docker build` honours `BUILDX_BUILDER` but ignores `docker buildx use`
/// and runs on the daemon's own "default" instance. Both routes are honoured now
/// that the build goes through `docker buildx build`.
///
/// What this case can and cannot catch, stated because the difference is not
/// obvious: on a daemon WITHOUT the containerd image store — the common case, and
/// GitHub's runners — the old pinned pass could not serve `type=oci` at all and
/// this fails outright. On a daemon WITH it (this repo's dev container) the docker
/// driver can serve the exporter, so the assertion that survives is the weaker
/// one: the build must report the builder it was told to use.
#[test]
fn build_honors_a_container_driver_builder_and_can_oci_export() {
    let builder = format!("deacon-t595-{}", std::process::id());

    let created = std::process::Command::new("docker")
        .args([
            "buildx",
            "create",
            "--name",
            &builder,
            "--driver",
            "docker-container",
        ])
        .output();
    match created {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            eprintln!(
                "skipping: could not create a docker-container builder ({})",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return;
        }
        Err(e) => {
            eprintln!("skipping: docker buildx unavailable ({e})");
            return;
        }
    }

    let temp_dir = TempDir::new().unwrap();
    write_devcontainer(&temp_dir, "FROM alpine:3.19\nLABEL deacon.test=build-595\n");
    let tar_path = temp_dir.path().join("oci-out.tar");

    let output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(&temp_dir)
        .env("BUILDX_BUILDER", &builder)
        .arg("build")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--output")
        .arg(format!("type=oci,dest={}", tar_path.display()))
        .arg("--output-format")
        .arg("json")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let _ = std::process::Command::new("docker")
        .args(["buildx", "rm", &builder])
        .output();

    if !output.status.success() && is_docker_unavailable(&stderr) {
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    assert!(
        output.status.success(),
        "an OCI export on the selected container-driver builder must succeed; stderr:\n{stderr}"
    );
    // The builder identity is the whole point: a pinned `default` would report the
    // docker driver here, and on a daemon without the containerd image store the
    // export would have failed outright.
    assert!(
        stderr.contains(&builder),
        "the build must run on the selected builder `{builder}`; stderr:\n{stderr}"
    );
    assert!(
        tar_path.exists(),
        "the --output build reported success but wrote no tar at {}",
        tar_path.display()
    );
}

/// A container-driver builder must also be able to export a LOCAL BUILD CACHE, on a
/// configuration that declares Features — the other half of the surface [#595] took
/// back, and the half its sibling above does not reach.
///
/// `build_honors_a_container_driver_builder_and_can_oci_export` covers OCI export on
/// a plain Dockerfile. This one differs on both axes that matter, and each was chosen
/// rather than varied for variety's sake:
///
/// * **`--cache-to type=local`** is what the old `--builder default` pin cost users
///   most concretely. The `docker` driver cannot serve a local cache export at all,
///   so a build silently redirected onto it produced no cache and still reported
///   success — the flag taken and dropped, which is the silent-fallback shape
///   constitution IV rules out. `parity/cases/build.json`'s `case-build-cache-to-flag`
///   deliberately narrowed to `type=inline` for exactly this reason and recorded that
///   local export was out of its reach; this is where that claim gets made.
/// * **A Feature** puts the build on the merged base + install-stage path
///   (`prepare_feature_layer` / `merge_dockerfile_with_feature_stage`), which is the
///   path that used to chain through a daemon-local tag and is therefore the path the
///   pin existed to prop up. A featureless build would exercise the easier half.
///
/// Upstream asserts exactly this pair — `should execute successfully and export
/// buildx cache with container builder`, over a Dockerfile-with-Features and an
/// image-with-Features config (`src/test/cli.build.test.ts` at v0.87.0). Both were
/// MEASURED to agree at oracle 0.87.0 before this landed: deacon and the reference
/// each exit 0 and each write `index.json` under the destination.
///
/// It is not a parity case because the harness has no vocabulary for creating a
/// buildx builder — an operation takes argv and fixtures, not host setup — and
/// inventing that primitive for one claim is a harness design decision, not a data
/// edit. The Docker-gated-test fallback is the same one #619 took for the Compose
/// `--image-name` claim, for the same kind of reason.
///
/// `BUILDX_BUILDER` rather than `docker buildx use`: the latter mutates host-global
/// state that concurrent tests share.
///
/// What each assertion is worth, stated because it is not uniform and its sibling
/// learned the same lesson: on a daemon WITHOUT the containerd image store — the
/// common case, and GitHub's runners — the docker driver cannot serve this exporter,
/// so the `index.json` assertion is the one that bites and a redirected build fails
/// it outright. On a daemon WITH it (this repo's dev container) the docker driver
/// CAN serve it: measured here, a deliberate `BUILDX_BUILDER=default` run of this
/// exact configuration exits 0 and writes `index.json` too. There the discriminating
/// assertion is the builder identity, which holds on every substrate. Both are kept
/// for that reason, and neither is redundant.
///
/// [#595]: https://github.com/get2knowio/deacon/issues/595
#[test]
fn build_honors_a_container_driver_builder_and_can_export_a_local_cache() {
    let builder = format!("deacon-t595-cache-{}", std::process::id());

    let created = std::process::Command::new("docker")
        .args([
            "buildx",
            "create",
            "--name",
            &builder,
            "--driver",
            "docker-container",
        ])
        .output();
    match created {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            eprintln!(
                "skipping: could not create a docker-container builder ({})",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return;
        }
        Err(e) => {
            eprintln!("skipping: docker buildx unavailable ({e})");
            return;
        }
    }

    let temp_dir = TempDir::new().unwrap();
    let dc = temp_dir.path().join(".devcontainer");
    let feature = dc.join("features/marker");
    fs::create_dir_all(&feature).unwrap();
    fs::write(dc.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
    fs::write(
        dc.join("devcontainer.json"),
        r#"{
    "name": "Container Builder Cache Export",
    "build": { "dockerfile": "Dockerfile" },
    "features": { "./features/marker": {} }
}
"#,
    )
    .unwrap();
    fs::write(
        feature.join("devcontainer-feature.json"),
        r#"{ "id": "marker", "version": "1.0.0", "name": "Marker" }"#,
    )
    .unwrap();
    // `/bin/sh`, not bash: the base is alpine, where a bash shebang exits 127.
    fs::write(
        feature.join("install.sh"),
        "#!/bin/sh\nset -e\necho installed > /usr/local/share/marker\n",
    )
    .unwrap();

    let cache_dir = temp_dir.path().join("build-cache");

    let output = Command::cargo_bin("deacon")
        .unwrap()
        .current_dir(&temp_dir)
        .env("BUILDX_BUILDER", &builder)
        .arg("build")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        // Without --force a previous run's cached image would satisfy this one and
        // no build — and so no cache export — would happen at all.
        .arg("--force")
        .arg("--cache-to")
        .arg(format!("type=local,dest={}", cache_dir.display()))
        .arg("--output-format")
        .arg("json")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    let _ = std::process::Command::new("docker")
        .args(["buildx", "rm", &builder])
        .output();

    if !output.status.success() && is_docker_unavailable(&stderr) {
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    assert!(
        output.status.success(),
        "a local cache export on the selected container-driver builder must succeed; \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("\"outcome\":\"success\""),
        "the result document must report success; stdout:\n{stdout}"
    );
    // The builder identity is half the claim: a pinned `default` would report the
    // docker driver here, which cannot serve this exporter at all.
    assert!(
        stderr.contains(&builder),
        "the build must run on the selected builder `{builder}`; stderr:\n{stderr}"
    );
    // The other half, and the one an `outcome: success` cannot give: the cache has
    // to actually be on disk. Upstream asserts on this same file.
    assert!(
        cache_dir.join("index.json").exists(),
        "the build reported success but exported no cache to {}",
        cache_dir.display()
    );

    // Best-effort: don't leave the produced image on the daemon.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        if let Some(name) = v.get("imageName").and_then(|n| {
            n.as_str()
                .map(str::to_string)
                .or_else(|| n.as_array()?.first()?.as_str().map(str::to_string))
        }) {
            let _ = std::process::Command::new("docker")
                .args(["rmi", "-f", &name])
                .output();
        }
    }
}

/// A base Dockerfile that ends on a **non-root `USER`, named by an `ARG`** must
/// still install Features, and the produced image must run as that user.
///
/// This is the one shape that failed both halves of the pipeline at once, and
/// each half was measured against the pinned reference (`@devcontainers/cli@0.87.0`)
/// on this exact fixture before the fix landed:
///
/// * [#685] — the generated install stage never switched to `root`, so
///   `install.sh` ran as `user2` and died on its first write outside `$HOME`.
///   The reference emits `USER root` before the Feature layers and restores the
///   image's user after. Measured: reference exit 0, deacon exit 1.
/// * [#686] — `find_user_statement` did no variable resolution, so a `USER $ARG`
///   resolved to nothing and every Feature was handed `_REMOTE_USER=root`.
///   Measured: reference `user2`, deacon `root`.
///
/// The assertion is on **image contents**, not the JSON outcome. That is
/// deliberate and is the lesson of #595 and #628: with the root switch missing the
/// build failed loudly, but with only the *user resolution* wrong it succeeded and
/// reported `outcome: success` while handing every Feature the wrong identity — a
/// result-document assertion cannot see that at all.
///
/// The user is named by an `ARG` rather than written literally so the test covers
/// both defects; a literal `USER user2` would exercise #685 alone.
///
/// [#685]: https://github.com/get2knowio/deacon/issues/685
/// [#686]: https://github.com/get2knowio/deacon/issues/686
#[test]
fn feature_install_becomes_root_and_restores_an_arg_named_dockerfile_user() {
    let temp_dir = TempDir::new().unwrap();
    let feature_dir = temp_dir.path().join(".devcontainer/marker");
    fs::create_dir_all(&feature_dir).unwrap();

    fs::write(
        temp_dir.path().join(".devcontainer/Dockerfile"),
        "FROM debian:bookworm-slim\n\
         RUN useradd -m user2\n\
         ARG IMAGE_USER=user2\n\
         USER $IMAGE_USER\n",
    )
    .unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        r#"{
    "name": "Feature Install User",
    "build": { "dockerfile": "Dockerfile" },
    "features": { "./marker": {} }
}
"#,
    )
    .unwrap();
    fs::write(
        feature_dir.join("devcontainer-feature.json"),
        r#"{ "id": "marker", "version": "1.0.0", "name": "marker" }"#,
    )
    .unwrap();
    // Writing to `/` is the point: it is what an unprivileged install cannot do.
    fs::write(
        feature_dir.join("install.sh"),
        "#!/usr/bin/env bash\nset -e\n\
         echo \"_REMOTE_USER=${_REMOTE_USER}\" > /marker.txt\n\
         echo \"_CONTAINER_USER=${_CONTAINER_USER}\" >> /marker.txt\n",
    )
    .unwrap();

    // A tempdir basename starts with `.`, which Docker rejects as a tag; keep
    // only the alphanumerics so the tag stays unique per run and legal.
    let suffix: String = temp_dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let tag = format!("deacon-test-featureuser:{suffix}");

    let output = Command::cargo_bin("deacon")
        .unwrap()
        .arg("build")
        .arg("--workspace-folder")
        .arg(temp_dir.path())
        .arg("--image-name")
        .arg(&tag)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() && is_docker_unavailable(&stderr) {
        eprintln!("skipping: docker unavailable ({})", stderr.trim());
        return;
    }

    // `docker run --entrypoint <bin> <image> [args...]` — the image comes before
    // the command's own arguments.
    let read_image = |entrypoint: &str, args: &[&str]| -> String {
        let out = std::process::Command::new("docker")
            .args(["run", "--rm", "--entrypoint", entrypoint])
            .arg(&tag)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let result = std::panic::catch_unwind(|| {
        assert!(
            output.status.success(),
            "a non-root USER in the base Dockerfile must not fail the Feature install (#685); \
             stderr:\n{stderr}"
        );

        let marker = read_image("cat", &["/marker.txt"]);
        assert!(
            marker.contains("_REMOTE_USER=user2"),
            "Features must be told the ARG-resolved user, not root (#686); got:\n{marker}"
        );
        assert!(
            marker.contains("_CONTAINER_USER=user2"),
            "_CONTAINER_USER must resolve the same way (#686); got:\n{marker}"
        );

        let whoami = read_image("whoami", &[]);
        assert_eq!(
            whoami, "user2",
            "the image must be handed back to its own user after the install (#685)"
        );
    });

    let _ = std::process::Command::new("docker")
        .args(["rmi", "-f", &tag])
        .output();

    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
