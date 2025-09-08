use deacon_core::config::ConfigLoader;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // Initialize tracing to see debug logs
    tracing_subscriber::fmt::init();
    
    let fixture_path = Path::new("fixtures/config/basic/devcontainer.jsonc");
    println!("Loading configuration from: {}", fixture_path.display());
    
    match ConfigLoader::load_from_path(fixture_path) {
        Ok(config) => {
            println!("✅ Successfully loaded configuration!");
            println!("Name: {:?}", config.name);
            println!("Image: {:?}", config.image);
            println!("Workspace folder: {:?}", config.workspace_folder);
            println!("Features: {}", serde_json::to_string_pretty(&config.features)?);
            println!("Container env vars: {:?}", config.container_env);
            println!("Forward ports: {:?}", config.forward_ports);
            println!("Run args: {:?}", config.run_args);
        }
        Err(e) => {
            println!("❌ Failed to load configuration: {}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}
