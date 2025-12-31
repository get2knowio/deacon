//! Plugin system for extending DevContainer CLI functionality
//!
//! This module provides a minimal plugin architecture enabling future dynamic extensions.
//! Plugins can augment configuration, register lifecycle hooks, and extend CLI functionality.
//!
//! ## Plugin Architecture
//!
//! The plugin system follows a static registry approach with compile-time feature flags:
//! - Plugins implement the `Plugin` trait with lifecycle hooks
//! - `PluginManager` maintains a static registry of plugins
//! - Plugins can augment DevContainer configuration during resolution
//! - Deterministic initialization and shutdown ordering
//!
//! ## References
//!
//! This implementation aligns with the extensibility requirements outlined in the CLI specification.

use crate::config::DevContainerConfig;
use crate::errors::Result;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tracing::{info, instrument, warn};

/// Plugin context provided during initialization
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// Plugin-specific configuration data
    pub config: HashMap<String, serde_json::Value>,
    /// Workspace root path
    pub workspace_root: Option<std::path::PathBuf>,
}

impl PluginContext {
    /// Create a new plugin context
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
            workspace_root: None,
        }
    }

    /// Set workspace root path
    pub fn with_workspace_root(mut self, workspace_root: std::path::PathBuf) -> Self {
        self.workspace_root = Some(workspace_root);
        self
    }

    /// Add plugin-specific configuration
    pub fn with_config(mut self, key: String, value: serde_json::Value) -> Self {
        self.config.insert(key, value);
        self
    }
}

impl Default for PluginContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Core plugin trait defining the lifecycle hooks for extensions
///
/// Plugins implement this trait to provide custom functionality:
/// - Configuration augmentation during resolution
/// - Initialization and cleanup lifecycle management
/// - Deterministic ordering through plugin names
pub trait Plugin: Send + Sync {
    /// Get the unique name of this plugin
    ///
    /// Plugin names are used for identification, ordering, and CLI selection.
    /// Names should be unique within the plugin registry.
    fn name(&self) -> &'static str;

    /// Initialize the plugin with the provided context
    ///
    /// Called during plugin manager initialization. Plugins should perform
    /// any required setup here.
    fn initialize(&self, ctx: &PluginContext) -> Result<()>;

    /// Shutdown the plugin and clean up resources
    ///
    /// Called during plugin manager shutdown. Plugins should release
    /// any resources and perform cleanup here.
    fn shutdown(&self) -> Result<()>;

    /// Augment the DevContainer configuration
    ///
    /// Optional hook allowing plugins to modify the configuration during
    /// resolution. This is called after base configuration loading but
    /// before validation.
    ///
    /// # Arguments
    ///
    /// * `config` - Mutable reference to the DevContainer configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if configuration augmentation fails.
    fn augment_config(&self, _config: &mut DevContainerConfig) -> Result<()> {
        // Default implementation does nothing
        Ok(())
    }
}

/// Plugin registry entry containing the plugin instance and metadata
struct PluginEntry {
    plugin: Box<dyn Plugin>,
    initialized: bool,
}

/// Static plugin registry storage
static PLUGIN_REGISTRY: OnceLock<Mutex<Vec<PluginEntry>>> = OnceLock::new();

/// Plugin manager providing registration and lifecycle management
///
/// The `PluginManager` maintains a static registry of plugins and provides
/// methods for registration, initialization, and shutdown. Plugins are
/// initialized in registration order and shut down in reverse order.
pub struct PluginManager;

impl PluginManager {
    /// Register a plugin with the manager
    ///
    /// Plugins are stored in registration order. Duplicate names are allowed
    /// but will generate warnings during initialization.
    ///
    /// # Arguments
    ///
    /// * `plugin` - Plugin instance to register
    ///
    /// # Examples
    ///
    /// ```rust
    /// use deacon_core::plugins::{Plugin, PluginManager, PluginContext};
    /// use deacon_core::config::DevContainerConfig;
    /// use deacon_core::errors::Result;
    ///
    /// struct ExamplePlugin;
    ///
    /// impl Plugin for ExamplePlugin {
    ///     fn name(&self) -> &'static str { "example" }
    ///     fn initialize(&self, _ctx: &PluginContext) -> Result<()> { Ok(()) }
    ///     fn shutdown(&self) -> Result<()> { Ok(()) }
    /// }
    ///
    /// PluginManager::register(Box::new(ExamplePlugin));
    /// ```
    pub fn register(plugin: Box<dyn Plugin>) {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let mut registry = registry.lock().unwrap();

        let plugin_name = plugin.name();

        // Check for duplicate names and warn
        if registry
            .iter()
            .any(|entry| entry.plugin.name() == plugin_name)
        {
            warn!("Plugin with name '{}' already registered", plugin_name);
        }

        registry.push(PluginEntry {
            plugin,
            initialized: false,
        });
    }

    /// Initialize all registered plugins
    ///
    /// Plugins are initialized in registration order. If any plugin fails
    /// to initialize, the process continues but an error is returned.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Plugin context for initialization
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all plugins initialize successfully, or an error
    /// describing the first failure encountered.
    #[instrument(skip(ctx))]
    pub fn initialize_all(ctx: &PluginContext) -> Result<()> {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let mut registry = registry.lock().unwrap();

        info!("Initializing {} registered plugins", registry.len());

        let mut first_error = None;

        for entry in registry.iter_mut() {
            let plugin_name = entry.plugin.name();

            if entry.initialized {
                warn!("Plugin '{}' already initialized, skipping", plugin_name);
                continue;
            }

            info!("Initializing plugin: {}", plugin_name);

            match entry.plugin.initialize(ctx) {
                Ok(()) => {
                    entry.initialized = true;
                    info!("Successfully initialized plugin: {}", plugin_name);
                }
                Err(e) => {
                    warn!("Failed to initialize plugin '{}': {}", plugin_name, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        info!("All plugins initialized successfully");
        Ok(())
    }

    /// Shutdown all initialized plugins
    ///
    /// Plugins are shut down in reverse initialization order. All plugins
    /// are attempted to be shut down even if some fail.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all plugins shut down successfully, or an error
    /// describing the first failure encountered.
    #[instrument]
    pub fn shutdown_all() -> Result<()> {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let mut registry = registry.lock().unwrap();

        let initialized_count = registry.iter().filter(|entry| entry.initialized).count();
        info!("Shutting down {} initialized plugins", initialized_count);

        let mut first_error = None;

        // Shutdown in reverse order
        for entry in registry.iter_mut().rev() {
            let plugin_name = entry.plugin.name();

            if !entry.initialized {
                continue;
            }

            info!("Shutting down plugin: {}", plugin_name);

            match entry.plugin.shutdown() {
                Ok(()) => {
                    entry.initialized = false;
                    info!("Successfully shut down plugin: {}", plugin_name);
                }
                Err(e) => {
                    warn!("Failed to shut down plugin '{}': {}", plugin_name, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        info!("All plugins shut down successfully");
        Ok(())
    }

    /// Apply configuration augmentation from all initialized plugins
    ///
    /// Calls `augment_config` on all initialized plugins in registration order.
    /// If any plugin fails, the process continues but an error is returned.
    ///
    /// # Arguments
    ///
    /// * `config` - Mutable reference to the DevContainer configuration
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all plugins successfully augment the configuration,
    /// or an error describing the first failure encountered.
    #[instrument(skip(config))]
    pub fn augment_config(config: &mut DevContainerConfig) -> Result<()> {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let registry = registry.lock().unwrap();

        let initialized_count = registry.iter().filter(|entry| entry.initialized).count();
        info!(
            "Applying configuration augmentation from {} plugins",
            initialized_count
        );

        let mut first_error = None;

        for entry in registry.iter() {
            if !entry.initialized {
                continue;
            }

            let plugin_name = entry.plugin.name();
            info!("Applying config augmentation from plugin: {}", plugin_name);

            match entry.plugin.augment_config(config) {
                Ok(()) => {
                    info!(
                        "Successfully applied config augmentation from plugin: {}",
                        plugin_name
                    );
                }
                Err(e) => {
                    warn!("Plugin '{}' failed to augment config: {}", plugin_name, e);
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if let Some(error) = first_error {
            return Err(error);
        }

        Ok(())
    }

    /// Get the names of all registered plugins
    ///
    /// Returns a vector of plugin names in registration order.
    pub fn plugin_names() -> Vec<String> {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let registry = registry.lock().unwrap();

        registry
            .iter()
            .map(|entry| entry.plugin.name().to_string())
            .collect()
    }

    /// Get the count of registered plugins
    pub fn plugin_count() -> usize {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let registry = registry.lock().unwrap();
        registry.len()
    }

    /// Clear all registered plugins (for testing)
    #[cfg(test)]
    pub fn clear_registry() {
        let registry = PLUGIN_REGISTRY.get_or_init(|| Mutex::new(Vec::new()));
        let mut registry = registry.lock().unwrap();
        registry.clear();
    }
}

/// Example NoOp plugin for testing and demonstration
///
/// This plugin demonstrates the plugin interface without performing any
/// actual functionality. It can be used for testing plugin registration,
/// initialization, and shutdown workflows.
#[derive(Debug)]
pub struct NoOpPlugin {
    name: &'static str,
}

impl NoOpPlugin {
    /// Create a new NoOp plugin with the given name
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl Plugin for NoOpPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn initialize(&self, _ctx: &PluginContext) -> Result<()> {
        info!("NoOp plugin '{}' initialized", self.name);
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        info!("NoOp plugin '{}' shut down", self.name);
        Ok(())
    }

    fn augment_config(&self, _config: &mut DevContainerConfig) -> Result<()> {
        info!("NoOp plugin '{}' augmented config", self.name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Test plugin that tracks initialization order
    #[derive(Debug)]
    struct TestPlugin {
        name: &'static str,
        init_counter: Arc<AtomicUsize>,
        init_order: Arc<Mutex<Vec<String>>>,
    }

    impl TestPlugin {
        fn new(
            name: &'static str,
            init_counter: Arc<AtomicUsize>,
            init_order: Arc<Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                name,
                init_counter,
                init_order,
            }
        }
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn initialize(&self, _ctx: &PluginContext) -> Result<()> {
            let order = self.init_counter.fetch_add(1, Ordering::SeqCst);
            self.init_order
                .lock()
                .unwrap()
                .push(format!("{}:{}", self.name, order));
            Ok(())
        }

        fn shutdown(&self) -> Result<()> {
            Ok(())
        }
    }

    /// Test plugin that modifies configuration
    #[derive(Debug)]
    struct ConfigAugmentingPlugin {
        name: &'static str,
        test_value: String,
    }

    impl ConfigAugmentingPlugin {
        fn new(name: &'static str, test_value: String) -> Self {
            Self { name, test_value }
        }
    }

    impl Plugin for ConfigAugmentingPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn initialize(&self, _ctx: &PluginContext) -> Result<()> {
            Ok(())
        }

        fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        fn augment_config(&self, config: &mut DevContainerConfig) -> Result<()> {
            // Modify the container environment for testing
            config
                .container_env
                .insert("PLUGIN_TEST".to_string(), self.test_value.clone());
            Ok(())
        }
    }

    #[test]
    fn test_plugin_registration() {
        PluginManager::clear_registry();

        let plugin1 = Box::new(NoOpPlugin::new("test1"));
        let plugin2 = Box::new(NoOpPlugin::new("test2"));

        PluginManager::register(plugin1);
        PluginManager::register(plugin2);

        assert_eq!(PluginManager::plugin_count(), 2);
        let names = PluginManager::plugin_names();
        assert_eq!(names, vec!["test1", "test2"]);
    }

    #[test]
    fn test_plugin_initialization_ordering() {
        PluginManager::clear_registry();

        let init_counter = Arc::new(AtomicUsize::new(0));
        let init_order = Arc::new(Mutex::new(Vec::new()));

        let plugin1 = Box::new(TestPlugin::new(
            "first",
            init_counter.clone(),
            init_order.clone(),
        ));
        let plugin2 = Box::new(TestPlugin::new(
            "second",
            init_counter.clone(),
            init_order.clone(),
        ));
        let plugin3 = Box::new(TestPlugin::new(
            "third",
            init_counter.clone(),
            init_order.clone(),
        ));

        PluginManager::register(plugin1);
        PluginManager::register(plugin2);
        PluginManager::register(plugin3);

        let ctx = PluginContext::new();
        PluginManager::initialize_all(&ctx).unwrap();

        let order = init_order.lock().unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], "first:0");
        assert_eq!(order[1], "second:1");
        assert_eq!(order[2], "third:2");

        PluginManager::shutdown_all().unwrap();
    }

    #[test]
    fn test_config_augmentation() {
        PluginManager::clear_registry();

        let plugin = Box::new(ConfigAugmentingPlugin::new(
            "config_test",
            "test_value".to_string(),
        ));

        PluginManager::register(plugin);

        let ctx = PluginContext::new();
        PluginManager::initialize_all(&ctx).unwrap();

        let mut config = DevContainerConfig {
            extends: None,
            name: Some("test".to_string()),
            image: Some("ubuntu".to_string()),
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: Vec::new(),
            features: serde_json::Value::Object(Default::default()),
            override_feature_install_order: None,
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: Vec::new(),
            container_env: std::collections::HashMap::new(),
            remote_env: std::collections::HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: Vec::new(),
            app_port: None,
            ports_attributes: std::collections::HashMap::new(),
            other_ports_attributes: None,
            run_args: Vec::new(),
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
            host_requirements: None,
            privileged: None,
            cap_add: Vec::new(),
            security_opt: Vec::new(),
        };

        PluginManager::augment_config(&mut config).unwrap();

        assert_eq!(
            config.container_env.get("PLUGIN_TEST"),
            Some(&"test_value".to_string())
        );

        PluginManager::shutdown_all().unwrap();
    }

    #[test]
    fn test_plugin_context() {
        let ctx = PluginContext::new()
            .with_workspace_root("/test/workspace".into())
            .with_config(
                "key1".to_string(),
                serde_json::Value::String("value1".to_string()),
            );

        assert_eq!(ctx.workspace_root, Some("/test/workspace".into()));
        assert_eq!(
            ctx.config.get("key1"),
            Some(&serde_json::Value::String("value1".to_string()))
        );
    }

    #[test]
    fn test_noop_plugin() {
        let plugin = NoOpPlugin::new("noop_test");

        assert_eq!(plugin.name(), "noop_test");

        let ctx = PluginContext::new();
        assert!(plugin.initialize(&ctx).is_ok());

        let mut config = DevContainerConfig {
            extends: None,
            name: Some("test".to_string()),
            image: Some("ubuntu".to_string()),
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: Vec::new(),
            features: serde_json::Value::Object(Default::default()),
            override_feature_install_order: None,
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: Vec::new(),
            container_env: std::collections::HashMap::new(),
            remote_env: std::collections::HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: Vec::new(),
            app_port: None,
            ports_attributes: std::collections::HashMap::new(),
            other_ports_attributes: None,
            run_args: Vec::new(),
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
            host_requirements: None,
            privileged: None,
            cap_add: Vec::new(),
            security_opt: Vec::new(),
        };

        assert!(plugin.augment_config(&mut config).is_ok());
        assert!(plugin.shutdown().is_ok());
    }
}
