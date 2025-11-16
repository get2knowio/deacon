use deacon_core::config::ConfigMerger;
use deacon_core::config::DevContainerConfig;
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn test_resolve_effective_config_merges_labels_and_substitutes() -> anyhow::Result<()> {
    let base = DevContainerConfig {
        workspace_folder: Some("${localWorkspaceFolder}/project".to_string()),
        remote_env: {
            let mut m = std::collections::HashMap::new();
            m.insert("BASE_VAR".to_string(), Some("base".to_string()));
            m.insert("EMPTY_VAR".to_string(), None);
            m
        },
        ..Default::default()
    };

    let mut labels = HashMap::new();
    labels.insert(
        "deacon.remoteEnv.BASE_VAR".to_string(),
        "label-override".to_string(),
    );
    labels.insert("deacon.remoteEnv/IGNORED".to_string(), "nope".to_string());
    labels.insert("other.label".to_string(), "value".to_string());

    let td = tempdir()?;
    let workspace_path = td.path();

    let (resolved, _report) =
        ConfigMerger::resolve_effective_config(&base, Some(&labels), workspace_path)?;

    // Workspace folder substitution should replace variable
    let wf = resolved.workspace_folder.unwrap();
    assert!(wf.ends_with("/project"));
    assert!(wf.contains(&workspace_path.canonicalize()?.to_string_lossy().to_string()));

    // Label should have overridden BASE_VAR
    assert_eq!(
        resolved
            .remote_env
            .get("BASE_VAR")
            .unwrap()
            .as_ref()
            .unwrap(),
        "label-override"
    );

    // EMPTY_VAR should be preserved as None
    assert!(resolved.remote_env.contains_key("EMPTY_VAR"));
    assert!(resolved.remote_env.get("EMPTY_VAR").unwrap().is_none());

    // Non-prefixed labels shouldn't be included (the label with slash shouldn't match prefix)
    assert!(!resolved.remote_env.contains_key("IGNORED"));

    Ok(())
}
