use assert_cmd::Command;

#[test]
fn hello_default() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("hello").assert().success();
}
