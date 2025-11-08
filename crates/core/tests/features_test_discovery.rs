use deacon_core::features_test::discovery;
use deacon_core::features_test::errors::Error;
use tempfile::tempdir;

#[test]
fn discover_test_collection_missing_dirs_returns_missing_directory() {
    // Create a temporary directory without src/ or test/
    let tmp = tempdir().expect("create temp dir");
    let path = tmp.path();

    // Ensure src and test do not exist
    let src = path.join("src");
    let test_dir = path.join("test");
    assert!(!src.exists());
    assert!(!test_dir.exists());

    let res = discovery::discover_test_collection(path);

    match res {
        Err(Error::MissingDirectory(msg)) => {
            assert!(msg.contains("src/") && msg.contains("test/"));
        }
        Err(e) => panic!("expected MissingDirectory, got: {:?}", e),
        Ok(_) => panic!("expected error for missing directories"),
    }
}
