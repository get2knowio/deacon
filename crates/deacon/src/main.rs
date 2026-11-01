use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod ui;

/// Stack size for the thread that drives the async runtime and for tokio's
/// worker threads.
///
/// Windows' default **main-thread** stack is ~1 MiB, versus ~8 MiB on Linux and
/// macOS. deacon's deeper subcommand call trees (config resolution, the doctor
/// diagnostics future, large async state machines) comfortably fit ~8 MiB but
/// overflow 1 MiB — so on Windows real subcommands crashed with
/// `STATUS_STACK_OVERFLOW` (0xC00000FD) while `--version`/`--help` survived
/// (clap short-circuits before that path). We run the whole app on an explicitly
/// large stack so behavior matches Unix on every platform.
const STACK_SIZE: usize = 16 * 1024 * 1024;

/// How long the runtime may wait for outstanding blocking work once the command is done.
const RUNTIME_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

fn main() -> Result<()> {
    // Drive everything on a thread with a generous, platform-independent stack.
    let child = std::thread::Builder::new()
        .name("deacon-main".to_string())
        .stack_size(STACK_SIZE)
        .spawn(run)?;
    child.join().unwrap_or_else(|_| std::process::exit(101)) // thread panicked
}

fn run() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(STACK_SIZE) // worker threads, too
        .build()?;
    let result = runtime.block_on(async_main());
    // Bound how long a finished command waits on leftover blocking work. Dropping the
    // runtime waits for every blocking thread with no limit, and on Windows tokio reads
    // child-process pipes on blocking threads. A pipe whose write end outlives the child we
    // killed (a Chocolatey/Scoop shim's grandchild still streaming `podman events`) never
    // returns EOF, so `up` printed its result and then never exited (#239). A timeout
    // rather than `shutdown_background` so an in-flight `tokio::fs` write still finishes.
    // The start-event gate now kills that whole process tree through a Job Object (#773),
    // so its pipes do reach EOF; this stays as the bound for any other child that leaks one.
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
    result
}

async fn async_main() -> Result<()> {
    // Parse CLI arguments
    let parsed = cli::Cli::parse();

    // Dispatch to CLI handler and handle special exit codes
    match parsed.dispatch().await {
        Ok(()) => Ok(()),
        Err(err) => {
            // Check for OutdatedExitCode (exit code 2 for --fail-on-outdated)
            if let Some(outdated_exit) = err.downcast_ref::<commands::outdated::OutdatedExitCode>()
            {
                std::process::exit(outdated_exit.0);
            }

            // Check if this is a NotImplemented error for special handling
            if let Some(deacon_error) = err.downcast_ref::<deacon_core::errors::DeaconError>() {
                if matches!(
                    deacon_error,
                    deacon_core::errors::DeaconError::Config(
                        deacon_core::errors::ConfigError::NotImplemented { .. }
                    )
                ) {
                    eprintln!("Error: {}", deacon_error);
                    std::process::exit(2); // Exit code 2 for NotImplemented
                }
            }

            // For all other errors, return them normally
            Err(err)
        }
    }
}
