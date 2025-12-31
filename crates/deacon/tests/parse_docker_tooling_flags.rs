use clap::Parser;
use deacon::cli::Cli;

#[test]
fn parses_docker_tooling_flags() {
    let args = vec![
        "deacon",
        "--docker-path",
        "/usr/bin/docker",
        "--docker-compose-path",
        "/usr/bin/docker-compose",
        "exec",
        "--container-id",
        "abc123",
        "--",
        "echo",
        "hi",
    ];

    let cli = Cli::parse_from(args);
    assert_eq!(cli.docker_path, "/usr/bin/docker");
    assert_eq!(cli.docker_compose_path, "/usr/bin/docker-compose");
    match cli.command {
        Some(deacon::cli::Commands::Exec {
            container_id,
            command,
            ..
        }) => {
            assert_eq!(container_id.unwrap(), "abc123");
            assert_eq!(command, vec!["echo".to_string(), "hi".to_string()]);
        }
        _ => panic!("Expected Exec command parsed"),
    }
}
