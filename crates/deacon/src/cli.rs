use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"), version, about = "Development container CLI (Rust reimplementation)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Say hello (placeholder example command)
    Hello {
        /// Name to greet
        #[arg(short, long, default_value = "world")]
        name: String,
    },
}

impl Cli {
    pub fn dispatch(self) -> Result<()> {
        match self.command {
            Commands::Hello { name } => {
                tracing::info!(%name, "hello command invoked");
                println!("Hello, {name}!");
            }
        }
        Ok(())
    }
}
