//! Runtime utilities for creating container runtimes from CLI context

use crate::cli::CliContext;
use anyhow::Result;
use deacon_core::runtime::{ContainerRuntimeImpl, RuntimeFactory};

/// Create a runtime instance based on CLI context
pub fn create_runtime_from_context(context: &CliContext) -> Result<ContainerRuntimeImpl> {
    let runtime_kind = RuntimeFactory::detect_runtime(context.runtime);
    Ok(RuntimeFactory::create_runtime(runtime_kind)?)
}

/// Create a runtime instance from optional CLI flag
pub fn create_runtime_from_flag(
    runtime_flag: Option<deacon_core::runtime::RuntimeKind>,
) -> Result<ContainerRuntimeImpl> {
    let runtime_kind = RuntimeFactory::detect_runtime(runtime_flag);
    Ok(RuntimeFactory::create_runtime(runtime_kind)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{CliContext, LogFormat, LogLevel, ProgressFormat};
    use deacon_core::runtime::RuntimeKind;

    #[test]
    fn test_create_runtime_from_context_default() {
        let context = CliContext {
            log_format: LogFormat::Text,
            log_level: LogLevel::Info,
            progress_format: ProgressFormat::Auto,
            progress_file: None,
            workspace_folder: None,
            config: None,
            override_config: None,
            secrets_files: vec![],
            no_redact: false,
            plugins: vec![],
            runtime: None,
        };

        let runtime = create_runtime_from_context(&context).unwrap();
        assert_eq!(runtime.runtime_name(), "docker");
    }

    #[test]
    fn test_create_runtime_from_context_podman() {
        let context = CliContext {
            log_format: LogFormat::Text,
            log_level: LogLevel::Info,
            progress_format: ProgressFormat::Auto,
            progress_file: None,
            workspace_folder: None,
            config: None,
            override_config: None,
            secrets_files: vec![],
            no_redact: false,
            plugins: vec![],
            runtime: Some(RuntimeKind::Podman),
        };

        let runtime = create_runtime_from_context(&context).unwrap();
        assert_eq!(runtime.runtime_name(), "podman");
    }

    #[test]
    fn test_create_runtime_from_flag() {
        let runtime = create_runtime_from_flag(Some(RuntimeKind::Docker)).unwrap();
        assert_eq!(runtime.runtime_name(), "docker");

        let runtime = create_runtime_from_flag(Some(RuntimeKind::Podman)).unwrap();
        assert_eq!(runtime.runtime_name(), "podman");

        let runtime = create_runtime_from_flag(None).unwrap();
        assert_eq!(runtime.runtime_name(), "docker");
    }
}
