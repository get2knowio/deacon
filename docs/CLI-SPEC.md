# DevContainer CLI Specification

## Overview

This specification defines the complete functionality and architecture of the DevContainer CLI, a Node.js-based command-line tool that implements the Development Containers Specification. The CLI enables creation, management, and execution of development containers from devcontainer.json configuration files.

## Table of Contents

1. [Architecture](#architecture)
2. [Command Line Interface](#command-line-interface)
3. [Configuration System](#configuration-system)
4. [Feature System](#feature-system)
5. [Template System](#template-system)
6. [Container Lifecycle Management](#container-lifecycle-management)
7. [Docker Integration](#docker-integration)
8. [Advanced Configuration Features](#advanced-configuration-features)
9. [OCI Registry Integration](#oci-registry-integration)
10. [HTTP and Network Handling](#http-and-network-handling)
11. [Process Management and Shell Integration](#process-management-and-shell-integration)
12. [Dotfiles Integration](#dotfiles-integration)
13. [Performance Optimization](#performance-optimization)
14. [Error Handling and Logging](#error-handling-and-logging)
15. [Security and Permissions](#security-and-permissions)
16. [Testing and Quality Assurance](#testing-and-quality-assurance)
17. [Monitoring and Observability](#monitoring-and-observability)
18. [Security and Compliance](#security-and-compliance)
19. [Extensibility and Plugin Architecture](#extensibility-and-plugin-architecture)
20. [Deployment and Distribution](#deployment-and-distribution)
21. [Migration and Compatibility](#migration-and-compatibility)
22. [Implementation Examples](#implementation-examples)
23. [Workflow Diagrams](#workflow-diagrams)

## Implementation Examples

### Complete Command Execution Flow

Here's a complete example of how the `devcontainer up` command is processed:

```typescript
// Example implementation of the 'up' command workflow
async function executeUpCommand(args: UpArgs): Promise<UpResult> {
  // 1. Initialize CLI host and parameters
  const cliHost = getCLIHost();
  const dockerParams = await createDockerParams({
    dockerPath: args.dockerPath,
    dockerComposePath: args.dockerComposePath,
    // ... other options
  });

  // 2. Resolve workspace and configuration
  const workspace = workspaceFromPath(cliHost.path, args.workspaceFolder);
  const configPath = await getDevContainerConfigPath(cliHost, workspace);
  const config = await readDevContainerConfigFile(cliHost, configPath);

  // 3. Apply variable substitution
  const substitutedConfig = substitute({
    platform: cliHost.platform,
    configFile: configPath,
    localWorkspaceFolder: workspace.rootFolderPath,
    env: cliHost.env,
  }, config);

  // 4. Handle container identification
  const idLabels = generateIdLabels(workspace, configPath);
  const existingContainer = await findContainerByLabels(dockerParams, idLabels);

  // 5. Create or reuse container
  let container: ContainerDetails;
  if (existingContainer && !args.removeExistingContainer) {
    container = existingContainer;
  } else {
    if (existingContainer) {
      await removeContainer(dockerParams, existingContainer.Id);
    }
    container = await createContainer(dockerParams, substitutedConfig, idLabels);
  }

  // 6. Install features and apply customizations
  await installFeatures(dockerParams, container, substitutedConfig.features);
  await applyCustomizations(dockerParams, container, substitutedConfig.customizations);

  // 7. Execute lifecycle commands
  await runLifecycleCommands(dockerParams, container, substitutedConfig, {
    onCreateCommand: substitutedConfig.onCreateCommand,
    postCreateCommand: substitutedConfig.postCreateCommand,
    postStartCommand: substitutedConfig.postStartCommand,
  });

  // 8. Set up user environment and dotfiles
  await setupUserEnvironment(dockerParams, container, substitutedConfig);
  if (args.dotfilesRepository) {
    await installDotfiles(dockerParams, container, {
      repository: args.dotfilesRepository,
      installCommand: args.dotfilesInstallCommand,
      targetPath: args.dotfilesTargetPath,
    });
  }

  return {
    containerId: container.Id,
    remoteUser: substitutedConfig.remoteUser,
    remoteWorkspaceFolder: substitutedConfig.workspaceFolder,
  };
}
```

### Feature Installation Example

```typescript
// Example feature installation process
async function installFeature(
  params: DockerCLIParameters,
  container: ContainerDetails,
  featureId: string,
  featureConfig: FeatureConfig
): Promise<void> {
  // 1. Parse feature identifier
  const featureRef = parseFeatureId(featureId);
  
  // 2. Download feature package
  const featurePackage = await downloadFeature(featureRef);
  
  // 3. Extract and validate
  const featurePath = await extractFeature(featurePackage);
  const featureMetadata = await readFeatureMetadata(featurePath);
  
  // 4. Process feature options
  const processedOptions = processFeatureOptions(
    featureMetadata.options,
    featureConfig
  );
  
  // 5. Execute installation script
  const installScript = path.join(featurePath, 'install.sh');
  await dockerExec(params, container.Id, {
    cmd: '/bin/bash',
    args: [installScript],
    env: {
      ...processedOptions,
      FEATURE_ID: featureId,
      FEATURE_PATH: '/tmp/dev-container-features/' + featureRef.id,
    },
    workingDir: '/tmp/dev-container-features/' + featureRef.id,
  });
  
  // 6. Apply feature configuration
  if (featureMetadata.containerEnv) {
    await setContainerEnvironment(params, container.Id, featureMetadata.containerEnv);
  }
  
  // 7. Execute feature lifecycle commands
  if (featureMetadata.postCreateCommand) {
    await runCommand(params, container.Id, featureMetadata.postCreateCommand);
  }
}
```

### Configuration Parsing Example

```typescript
// Example configuration parsing and merging
async function parseAndMergeConfiguration(
  cliHost: CLIHost,
  configPath: URI,
  overrides?: Partial<DevContainerConfig>
): Promise<MergedDevContainerConfig> {
  // 1. Read configuration file
  const configContent = await readFile(cliHost, configPath);
  const rawConfig = jsonc.parse(configContent) as DevContainerConfig;
  
  // 2. Apply schema validation
  const validationErrors = validateConfiguration(rawConfig);
  if (validationErrors.length > 0) {
    throw new ContainerError(`Configuration validation failed: ${validationErrors.join(', ')}`);
  }
  
  // 3. Process extends property
  let baseConfig: DevContainerConfig = {};
  if (rawConfig.extends) {
    baseConfig = await resolveExtendsConfiguration(cliHost, configPath, rawConfig.extends);
  }
  
  // 4. Merge configurations in order: base -> raw -> overrides
  const mergedConfig = mergeConfigurations([
    getDefaultConfiguration(),
    baseConfig,
    rawConfig,
    overrides || {},
  ]);
  
  // 5. Resolve image metadata if applicable
  let imageMetadata: ImageMetadata = {};
  if (isImageConfig(mergedConfig) && mergedConfig.image) {
    imageMetadata = await getImageMetadata(mergedConfig.image);
  }
  
  // 6. Final merge with image metadata
  return mergeConfiguration(mergedConfig, imageMetadata);
}
```

## Workflow Diagrams

### 1. Container Creation Workflow

```
[Start] → [Parse Args] → [Load Config] → [Substitute Variables]
   ↓
[Check Existing Container] → [Remove if needed] → [Create Container]
   ↓
[Install Features] → [Apply Customizations] → [Run Lifecycle Commands]
   ↓
[Setup Environment] → [Install Dotfiles] → [Return Result]
```

### 2. Feature Installation Workflow

```
[Feature Request] → [Parse Feature ID] → [Resolve Version]
   ↓
[Check Cache] → [Download if needed] → [Extract Package]
   ↓
[Validate Metadata] → [Process Dependencies] → [Install in Order]
   ↓
[Run Install Script] → [Apply Configuration] → [Execute Hooks]
```

### 3. Configuration Resolution Workflow

```
[Config Path] → [Read File] → [Parse JSON] → [Validate Schema]
   ↓
[Resolve Extends] → [Merge Base Config] → [Apply Overrides]
   ↓
[Variable Substitution] → [Image Metadata] → [Final Merge]
```

### 4. Container Execution Workflow

```
[Exec Command] → [Find Container] → [Probe Environment]
   ↓
[Apply Remote User] → [Set Working Dir] → [Execute Command]
   ↓
[Stream Output] → [Handle Signals] → [Return Exit Code]
```

### 5. Build Process Workflow

```
[Build Request] → [Resolve Build Context] → [Process Dockerfile]
   ↓
[Install Features] → [Apply Build Args] → [Execute Build]
   ↓
[Apply Labels] → [Tag Image] → [Push if needed]
```

## Detailed API Specifications

### Core Interfaces

```typescript
// Main CLI entry point interface
interface DevContainerCLI {
  up(options: UpOptions): Promise<UpResult>;
  build(options: BuildOptions): Promise<BuildResult>;
  exec(options: ExecOptions): Promise<ExecResult>;
  runUserCommands(options: RunUserCommandsOptions): Promise<void>;
  readConfiguration(options: ReadConfigurationOptions): Promise<ConfigurationResult>;
  features: FeaturesCLI;
  templates: TemplatesCLI;
}

// Feature management interface
interface FeaturesCLI {
  test(options: TestOptions): Promise<TestResult>;
  package(options: PackageOptions): Promise<PackageResult>;
  publish(options: PublishOptions): Promise<PublishResult>;
  info(options: InfoOptions): Promise<InfoResult>;
  resolveDependencies(options: ResolveDependenciesOptions): Promise<DependencyResult>;
  generateDocs(options: GenerateDocsOptions): Promise<void>;
}

// Template management interface
interface TemplatesCLI {
  apply(options: ApplyOptions): Promise<ApplyResult>;
  publish(options: PublishOptions): Promise<PublishResult>;
  metadata(options: MetadataOptions): Promise<MetadataResult>;
  generateDocs(options: GenerateDocsOptions): Promise<void>;
}
```

### Configuration Interfaces

```typescript
// Complete configuration interface
interface DevContainerConfig {
  // Base properties
  name?: string;
  image?: string;
  dockerFile?: string;
  build?: BuildConfiguration;
  
  // Container runtime
  runArgs?: string[];
  shutdownAction?: 'none' | 'stopContainer' | 'stopCompose';
  overrideCommand?: boolean;
  init?: boolean;
  privileged?: boolean;
  capAdd?: string[];
  securityOpt?: string[];
  
  // Workspace configuration
  workspaceFolder?: string;
  workspaceMount?: string;
  mounts?: (Mount | string)[];
  
  // Environment configuration
  containerEnv?: Record<string, string>;
  remoteEnv?: Record<string, string | null>;
  containerUser?: string;
  remoteUser?: string;
  updateRemoteUserUID?: boolean;
  userEnvProbe?: UserEnvProbe;
  
  // Network configuration
  forwardPorts?: (number | string)[];
  appPort?: number | string | (number | string)[];
  portsAttributes?: Record<string, PortAttributes>;
  otherPortsAttributes?: PortAttributes;
  
  // Lifecycle commands
  initializeCommand?: string | string[];
  onCreateCommand?: string | string[];
  updateContentCommand?: string | string[];
  postCreateCommand?: string | string[];
  postStartCommand?: string | string[];
  postAttachCommand?: string | string[];
  waitFor?: DevContainerConfigCommand;
  
  // Features and customizations
  features?: Record<string, FeatureConfiguration>;
  overrideFeatureInstallOrder?: string[];
  customizations?: Record<string, any>;
  
  // Host requirements
  hostRequirements?: HostRequirements;
}
```

### Docker Integration Interfaces

```typescript
// Docker CLI abstraction
interface DockerCLI {
  // Container operations
  createContainer(options: CreateContainerOptions): Promise<ContainerDetails>;
  startContainer(containerId: string): Promise<void>;
  stopContainer(containerId: string, timeout?: number): Promise<void>;
  removeContainer(containerId: string, force?: boolean): Promise<void>;
  inspectContainer(containerId: string): Promise<ContainerDetails>;
  listContainers(filters?: ContainerFilter): Promise<ContainerDetails[]>;
  
  // Image operations
  buildImage(options: BuildImageOptions): Promise<BuildResult>;
  pullImage(imageName: string, options?: PullOptions): Promise<void>;
  inspectImage(imageName: string): Promise<ImageDetails>;
  tagImage(sourceImage: string, targetImage: string): Promise<void>;
  pushImage(imageName: string, options?: PushOptions): Promise<void>;
  
  // Execution operations
  exec(containerId: string, options: ExecOptions): Promise<ExecResult>;
  logs(containerId: string, options?: LogOptions): Promise<string>;
  
  // Network operations
  createNetwork(options: NetworkOptions): Promise<NetworkDetails>;
  connectToNetwork(containerId: string, networkId: string): Promise<void>;
  
  // Volume operations
  createVolume(options: VolumeOptions): Promise<VolumeDetails>;
  mountVolume(containerId: string, volumeId: string, mountPoint: string): Promise<void>;
}
```

## Error Handling Patterns

### Error Classification

```typescript
// Error type hierarchy
abstract class DevContainerError extends Error {
  abstract readonly code: string;
  abstract readonly category: ErrorCategory;
}

enum ErrorCategory {
  Configuration = 'configuration',
  Docker = 'docker',
  Network = 'network',
  Feature = 'feature',
  Template = 'template',
  FileSystem = 'filesystem',
  Authentication = 'authentication',
  Validation = 'validation',
}

// Specific error types
class ConfigurationError extends DevContainerError {
  readonly code = 'CONFIGURATION_ERROR';
  readonly category = ErrorCategory.Configuration;
}

class DockerError extends DevContainerError {
  readonly code = 'DOCKER_ERROR';
  readonly category = ErrorCategory.Docker;
  constructor(message: string, public readonly dockerCode?: string) {
    super(message);
  }
}

class FeatureInstallationError extends DevContainerError {
  readonly code = 'FEATURE_INSTALLATION_ERROR';
  readonly category = ErrorCategory.Feature;
  constructor(message: string, public readonly featureId: string) {
    super(message);
  }
}
```

### Error Recovery Strategies

```typescript
// Retry configuration
interface RetryConfig {
  maxAttempts: number;
  baseDelay: number;
  maxDelay: number;
  exponentialBase: number;
  jitter: boolean;
}

// Retry implementation
async function withRetry<T>(
  operation: () => Promise<T>,
  config: RetryConfig,
  shouldRetry: (error: Error) => boolean = () => true
): Promise<T> {
  let lastError: Error;
  
  for (let attempt = 1; attempt <= config.maxAttempts; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      
      if (attempt === config.maxAttempts || !shouldRetry(error)) {
        throw error;
      }
      
      const delay = calculateRetryDelay(attempt, config);
      await sleep(delay);
    }
  }
  
  throw lastError!;
}
```

## Performance Considerations

### Caching Strategies

```typescript
// Cache interface
interface Cache<T> {
  get(key: string): Promise<T | undefined>;
  set(key: string, value: T, ttl?: number): Promise<void>;
  delete(key: string): Promise<void>;
  clear(): Promise<void>;
  size(): Promise<number>;
}

// Multi-level cache implementation
class MultiLevelCache<T> implements Cache<T> {
  constructor(
    private readonly memoryCache: Cache<T>,
    private readonly diskCache: Cache<T>
  ) {}
  
  async get(key: string): Promise<T | undefined> {
    // Try memory cache first
    let value = await this.memoryCache.get(key);
    if (value !== undefined) {
      return value;
    }
    
    // Fall back to disk cache
    value = await this.diskCache.get(key);
    if (value !== undefined) {
      // Promote to memory cache
      await this.memoryCache.set(key, value);
      return value;
    }
    
    return undefined;
  }
  
  async set(key: string, value: T, ttl?: number): Promise<void> {
    await Promise.all([
      this.memoryCache.set(key, value, ttl),
      this.diskCache.set(key, value, ttl),
    ]);
  }
}
```

### Parallel Execution

```typescript
// Parallel execution with concurrency control
async function executeWithConcurrency<T, R>(
  items: T[],
  executor: (item: T) => Promise<R>,
  maxConcurrency: number
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  const executing: Promise<void>[] = [];
  
  for (let i = 0; i < items.length; i++) {
    const promise = executor(items[i]).then(result => {
      results[i] = result;
    });
    
    executing.push(promise);
    
    if (executing.length >= maxConcurrency) {
      await Promise.race(executing);
      // Remove completed promises
      for (let j = executing.length - 1; j >= 0; j--) {
        if (isPromiseResolved(executing[j])) {
          executing.splice(j, 1);
        }
      }
    }
  }
  
  await Promise.all(executing);
  return results;
}
```

This comprehensive specification provides the complete architectural and functional definition of the DevContainer CLI, enabling reconstruction in any programming language while maintaining full compatibility with the Development Containers Specification. The specification covers all major components, data structures, workflows, and implementation details necessary for a complete reimplementation.

## Architecture

### Core Components

1. **Entry Point**: `devcontainer.js` - Executable wrapper that loads the compiled CLI
2. **Main CLI Module**: `src/spec-node/devContainersSpecCLI.ts` - Command parser and dispatcher
3. **Configuration System**: `src/spec-configuration/` - Handles parsing and validation
4. **Container Management**: `src/spec-node/` - Docker integration and container lifecycle
5. **Common Utilities**: `src/spec-common/` - Shared functionality across components
6. **Feature System**: `src/spec-node/featuresCLI/` - Feature management and installation
7. **Template System**: `src/spec-node/templatesCLI/` - Template application and publishing

### Module Structure

```
src/
├── spec-common/          # Shared utilities and common functionality
│   ├── injectHeadless.ts  # Lifecycle management and user environment probing
│   ├── commonUtils.ts     # CLI host abstraction and utilities
│   ├── errors.ts          # Error handling and types
│   ├── shellServer.ts     # Shell execution server
│   └── variableSubstitution.ts  # Variable substitution engine
├── spec-configuration/    # Configuration parsing and management
│   ├── configuration.ts   # Core configuration types and parsing
│   ├── containerFeaturesConfiguration.ts  # Feature configuration
│   ├── configurationCommonUtils.ts  # Common configuration utilities
│   └── containerCollectionsOCI.ts  # OCI registry integration
├── spec-node/            # Node.js-specific implementation
│   ├── devContainersSpecCLI.ts  # Main CLI entry point
│   ├── devContainers.ts  # Core container lifecycle operations
│   ├── singleContainer.ts  # Single container management
│   ├── dockerCompose.ts  # Docker Compose integration
│   ├── containerFeatures.ts  # Feature installation and management
│   ├── utils.ts          # Node-specific utilities
│   ├── featuresCLI/      # Feature command implementations
│   └── templatesCLI/     # Template command implementations
├── spec-shutdown/        # Docker CLI integration
│   └── dockerUtils.ts    # Docker command execution and utilities
└── spec-utils/           # General utilities
    ├── log.ts            # Logging system
    ├── workspaces.ts     # Workspace management
    └── event.ts          # Event system
```

## Command Line Interface

### Global Options

The CLI uses `yargs` for argument parsing with the following global configuration:
- Script name: `devcontainer`
- Halt at non-option for `exec` command
- Boolean negation disabled (to support `--no-cache`)
- Terminal width-aware help text (max 120 characters)

### Commands

#### 1. `up` - Create and Run Dev Container

**Purpose**: Creates and starts a development container from configuration.

**Syntax**: `devcontainer up [options]`

**Key Options**:
```bash
--workspace-folder <path>              # Workspace folder path
--config <path>                       # devcontainer.json path
--override-config <path>              # Override configuration file
--docker-path <path>                  # Docker CLI path
--docker-compose-path <path>          # Docker Compose CLI path
--container-data-folder <path>        # Container data folder
--mount-workspace-git-root           # Mount workspace using Git root
--id-label <name=value>              # Container identification labels
--remove-existing-container          # Remove existing container first
--build-no-cache                     # Build with --no-cache
--expect-existing-container          # Fail if container doesn't exist
--skip-post-create                   # Skip lifecycle commands
--skip-non-blocking-commands         # Skip non-blocking commands
--prebuild                           # Stop after build commands
--additional-features <json>         # Additional features to install
--dotfiles-repository <url>          # Dotfiles repository URL
--secrets-file <path>                # Secret environment variables file
```

**Workflow**:
1. Parse command arguments and validate options
2. Resolve configuration file (devcontainer.json)
3. Parse and merge configuration with defaults
4. Handle container identification and lifecycle
5. Execute container creation or reuse existing
6. Install features and apply customizations
7. Run lifecycle commands (onCreate, postCreate, etc.)
8. Set up user environment and dotfiles
9. Return container information

#### 2. `set-up` - Set Up Existing Container

**Purpose**: Configures an existing container as a development container.

**Syntax**: `devcontainer set-up [options]`

**Key Functionality**:
- Applies dev container configuration to running container
- Installs features and customizations
- Runs lifecycle commands
- Sets up user environment

#### 3. `build [path]` - Build Dev Container Image

**Purpose**: Builds a container image from devcontainer configuration.

**Syntax**: `devcontainer build [path] [options]`

**Key Options**:
```bash
--image-name <name>                   # Output image name
--no-cache                           # Disable build cache
--cache-from <image>                 # Cache source images
--cache-to <image>                   # Cache destination
--buildkit <auto|never>              # BuildKit usage control
--platform <platform>               # Target platform
--push                               # Push image after build
--output <type=dest>                 # Build output destination
```

**Build Process**:
1. Resolve build context and configuration
2. Process Dockerfile or docker-compose setup
3. Install and configure features
4. Execute build-time lifecycle commands
5. Apply metadata labels
6. Build final image
7. Optionally push to registry

#### 4. `run-user-commands` - Execute Lifecycle Commands

**Purpose**: Runs user-defined lifecycle commands in container.

**Syntax**: `devcontainer run-user-commands [options]`

**Lifecycle Commands** (in order):
1. `initializeCommand` - Run before container creation
2. `onCreateCommand` - Run when container is created
3. `updateContentCommand` - Run when content is updated
4. `postCreateCommand` - Run after container creation
5. `postStartCommand` - Run after container starts
6. `postAttachCommand` - Run when attaching to container

**Key Options**:
```bash
--skip-post-create                   # Skip postCreateCommand
--skip-post-attach                   # Skip postAttachCommand
--skip-non-blocking-commands         # Stop at waitFor command
--prebuild                           # Stop after updateContentCommand
--stop-for-personalization          # Stop before personalization
```

#### 5. `read-configuration` - Parse and Display Configuration

**Purpose**: Reads and outputs the merged devcontainer configuration.

**Syntax**: `devcontainer read-configuration [options]`

**Output Options**:
```bash
--include-configuration              # Include raw configuration
--include-merged-configuration       # Include merged configuration
--output-format <text|json>          # Output format
```

#### 6. `exec <cmd> [args...]` - Execute Command in Container

**Purpose**: Executes commands in a running development container.

**Syntax**: `devcontainer exec [options] <command> [arguments...]`

**Key Features**:
- Applies user environment variables
- Uses correct remote user
- Supports TTY allocation
- Environment probing integration

#### 7. `features` - Feature Management Commands

**Purpose**: Manage development container features.

**Subcommands**:

##### `features test [target]`
- Tests feature implementations
- Validates feature configuration
- Runs feature test scenarios

##### `features package <target>`
- Packages features for distribution
- Creates OCI-compatible artifacts
- Validates feature metadata

##### `features publish <target>`
- Publishes features to OCI registry
- Handles authentication and pushing
- Updates metadata and manifests

##### `features info <mode> <feature>`
- Retrieves feature metadata
- Modes: `manifest`, `tags`, `dependencies`, `verbose`
- Supports both local and remote features

##### `features resolve-dependencies`
- Analyzes feature dependency graphs
- Resolves installation order
- Detects circular dependencies

##### `features generate-docs`
- Generates documentation from feature metadata
- Creates README files and reference docs

#### 8. `templates` - Template Management Commands

**Purpose**: Manage development container templates.

**Subcommands**:

##### `templates apply`
- Applies template to current project
- Merges template configuration
- Handles template customization

##### `templates publish <target>`
- Publishes templates to OCI registry
- Packages template files and metadata

##### `templates metadata <templateId>`
- Retrieves template metadata
- Shows available options and configurations

##### `templates generate-docs`
- Generates template documentation

#### 9. `outdated` - Version Information

**Purpose**: Shows current and available versions of features and dependencies.

#### 10. `upgrade` - Upgrade Lockfile

**Purpose**: Updates lockfile with latest compatible versions.

## Configuration System

### DevContainer Configuration Types

#### Base Configuration Interface
```typescript
interface DevContainerConfig {
  configFilePath?: URI;
  name?: string;
  forwardPorts?: (number | string)[];
  appPort?: number | string | (number | string)[];
  portsAttributes?: Record<string, PortAttributes>;
  otherPortsAttributes?: PortAttributes;
  runArgs?: string[];
  shutdownAction?: 'none' | 'stopContainer';
  overrideCommand?: boolean;
  
  // Lifecycle commands
  initializeCommand?: string | string[];
  onCreateCommand?: string | string[];
  updateContentCommand?: string | string[];
  postCreateCommand?: string | string[];
  postStartCommand?: string | string[];
  postAttachCommand?: string | string[];
  waitFor?: DevContainerConfigCommand;
  
  // Container configuration
  workspaceFolder?: string;
  workspaceMount?: string;
  mounts?: (Mount | string)[];
  containerEnv?: Record<string, string>;
  containerUser?: string;
  init?: boolean;
  privileged?: boolean;
  capAdd?: string[];
  securityOpt?: string[];
  
  // User environment
  remoteEnv?: Record<string, string | null>;
  remoteUser?: string;
  updateRemoteUserUID?: boolean;
  userEnvProbe?: UserEnvProbe;
  
  // Features and customizations
  features?: Record<string, string | boolean | Record<string, string | boolean>>;
  overrideFeatureInstallOrder?: string[];
  hostRequirements?: HostRequirements;
  customizations?: Record<string, any>;
}
```

#### Configuration Variants

##### 1. Image-based Configuration
```typescript
interface DevContainerFromImageConfig extends DevContainerConfig {
  image?: string; // Base container image
}
```

##### 2. Dockerfile-based Configuration
```typescript
interface DevContainerFromDockerfileConfig extends DevContainerConfig {
  dockerFile?: string; // Path to Dockerfile
  context?: string;    // Build context
  build?: {
    target?: string;
    args?: Record<string, string>;
    cacheFrom?: string | string[];
    options?: string[];
  };
}
```

##### 3. Docker Compose Configuration
```typescript
interface DevContainerFromDockerComposeConfig {
  dockerComposeFile: string | string[];
  service: string;
  workspaceFolder: string;
  runServices?: string[];
  shutdownAction?: 'none' | 'stopCompose';
  // ... other shared properties
}
```

### Configuration Resolution

The configuration system follows this resolution order:

1. **Command-line overrides** - Explicit `--override-config` option
2. **Workspace configuration** - `.devcontainer/devcontainer.json`
3. **Fallback configuration** - `.devcontainer.json` in workspace root
4. **Default values** - Built-in defaults for optional properties

### Variable Substitution

The CLI supports variable substitution in configuration values:

**Supported Variables**:
- `${localWorkspaceFolder}` - Local workspace folder path
- `${containerWorkspaceFolder}` - Container workspace folder path
- `${localEnv:VARIABLE_NAME}` - Local environment variable
- `${containerEnv:VARIABLE_NAME}` - Container environment variable

**Substitution Contexts**:
- **Before container creation**: Local environment and workspace
- **After container creation**: Container environment and properties

## Feature System

### Feature Architecture

Features are reusable container setup components that can be installed in development containers.

#### Feature Definition Structure
```typescript
interface Feature {
  // Metadata
  id: string;
  version?: string;
  name?: string;
  description?: string;
  documentationURL?: string;
  licenseURL?: string;
  
  // Configuration
  options?: Record<string, FeatureOption>;
  containerEnv?: Record<string, string>;
  mounts?: Mount[];
  init?: boolean;
  privileged?: boolean;
  capAdd?: string[];
  securityOpt?: string[];
  entrypoint?: string;
  customizations?: VSCodeCustomizations;
  
  // Dependencies and ordering
  installsAfter?: string[];
  dependsOn?: Record<string, string | boolean | Record<string, string | boolean>>;
  
  // Lifecycle hooks
  onCreateCommand?: string | string[];
  updateContentCommand?: string | string[];
  postCreateCommand?: string | string[];
  postStartCommand?: string | string[];
  postAttachCommand?: string | string[];
  
  // Internal properties
  cachePath?: string;
  internalVersion?: string;
  consecutiveId?: string;
  value: boolean | string | Record<string, boolean | string | undefined>;
  currentId?: string;
  included: boolean;
}
```

#### Feature Option Types
```typescript
type FeatureOption = {
  type: 'boolean';
  default?: boolean;
  description?: string;
} | {
  type: 'string';
  enum?: string[];
  default?: string;
  description?: string;
} | {
  type: 'string';
  proposals?: string[];
  default?: string;
  description?: string;
};
```

### Feature Installation Process

1. **Resolution**: Parse feature identifiers and resolve to specific versions
2. **Dependency Analysis**: Build dependency graph and determine installation order
3. **Download**: Fetch feature packages from OCI registries or local sources
4. **Validation**: Validate feature metadata and compatibility
5. **Installation**: Execute feature installation scripts in dependency order
6. **Configuration**: Apply feature configuration to container
7. **Lifecycle Execution**: Run feature lifecycle commands

### Feature Distribution

Features are distributed as OCI-compatible packages with:
- **Manifest**: Feature metadata and configuration
- **Install Script**: Installation logic (typically `install.sh`)
- **Assets**: Additional files and resources

## Container Lifecycle Management

### Container Resolution Process

1. **Configuration Loading**
   - Parse devcontainer.json configuration
   - Apply variable substitution
   - Merge with image metadata
   - Validate configuration schema

2. **Container Discovery**
   - Check for existing containers using ID labels
   - Evaluate container state and compatibility
   - Decide on reuse vs. recreation

3. **Image Preparation**
   - Build or pull base image
   - Install and configure features
   - Apply container customizations
   - Tag with metadata labels

4. **Container Creation**
   - Apply run arguments and configuration
   - Set up mounts and volumes
   - Configure networking and ports
   - Start container with appropriate settings

5. **Environment Setup**
   - Probe user environment variables
   - Set up remote user configuration
   - Apply environment variable mappings
   - Configure shell and terminal settings

6. **Lifecycle Execution**
   - Run initialization commands
   - Execute creation and setup commands
   - Apply post-creation customizations
   - Install dotfiles if configured

### User Environment Probing

The CLI implements sophisticated user environment detection:

**Probe Types**:
- `none`: No environment probing
- `loginInteractiveShell`: Full login shell initialization
- `interactiveShell`: Interactive shell without login
- `loginShell`: Login shell without interaction

**Probing Process**:
1. Execute shell with specified mode
2. Capture environment variable output
3. Parse and merge with container environment
4. Cache results for performance
5. Apply PATH merging logic

### Lifecycle Commands

Commands are executed in specific order with proper error handling:

1. **initializeCommand**: Host-side initialization
2. **onCreateCommand**: Container creation setup
3. **updateContentCommand**: Content synchronization
4. **postCreateCommand**: Post-creation configuration
5. **postStartCommand**: Container startup tasks
6. **postAttachCommand**: Attachment preparation

**Execution Context**:
- Commands run with proper user context
- Environment variables are fully resolved
- Working directory is set correctly
- Output is captured and logged

## Docker Integration

### Docker CLI Abstraction

The CLI provides abstraction over Docker operations:

```typescript
interface DockerCLIParameters {
  cliHost: CLIHost;
  dockerCLI: string;
  dockerComposeCLI: string;
  env: NodeJS.ProcessEnv;
  output: Log;
}
```

### Container Operations

#### Container Creation
- Applies run arguments and labels
- Sets up volume mounts and binds
- Configures network settings
- Handles user and permission mapping

#### Image Building
- Supports both Dockerfile and Docker Compose builds
- Integrates with BuildKit when available
- Handles multi-stage builds and caching
- Applies feature installations during build

#### Container Management
- Start, stop, and restart operations
- Exec command execution
- Log retrieval and monitoring
- Health checking and status

### Docker Compose Integration

For Docker Compose configurations:

1. **Compose File Processing**
   - Parse docker-compose.yml files
   - Apply devcontainer overrides
   - Generate override configurations
   - Handle service dependencies

2. **Service Management**
   - Start specified services
   - Apply container customizations
   - Handle volume and network setup
   - Manage service lifecycle

## Error Handling and Logging

### Error Types

```typescript
class ContainerError extends Error {
  constructor(message: string, public readonly code?: string) {
    super(message);
  }
}
```

### Logging System

The CLI implements structured logging with multiple output formats:

**Log Levels**:
- `trace`: Detailed debugging information
- `debug`: Debug-level information
- `info`: General information
- `warning`: Warning messages
- `error`: Error messages

**Log Formats**:
- `text`: Human-readable text format
- `json`: Structured JSON format

**Log Outputs**:
- Terminal output with color support
- File logging for debugging
- Progress indicators for long operations

### Progress Tracking

```typescript
enum ResolverProgress {
  Begin,
  CloningRepository,
  BuildingImage,
  StartingContainer,
  InstallingServer,
  StartingServer,
  End,
}
```

## Security and Permissions

### User Mapping

The CLI handles user ID mapping between host and container:

1. **UID/GID Detection**: Detect host user IDs
2. **Container User Setup**: Configure container user
3. **Permission Mapping**: Map file permissions
4. **Privilege Handling**: Handle privileged operations

### Security Options

- Support for security options (`--security-opt`)
- Capability management (`--cap-add`, `--cap-drop`)
- Privileged container handling
- AppArmor and SELinux integration

## Testing and Validation

### Test Structure

The CLI includes comprehensive tests:

1. **Unit Tests**: Individual component testing
2. **Integration Tests**: End-to-end workflow testing
3. **Feature Tests**: Feature installation and validation
4. **CLI Tests**: Command-line interface testing

### Validation Framework

- Configuration schema validation
- Feature compatibility checking
- Container state validation
- Output format validation

## Platform Support

### Supported Platforms

- **Linux**: Full native support
- **macOS**: Native Docker Desktop integration
- **Windows**: WSL2 and Docker Desktop support

### Platform-Specific Handling

- Path resolution and conversion
- File system permissions
- Network configuration
- Process management

## Extension Points

### Custom CLI Hosts

The CLI supports custom CLI host implementations:

```typescript
interface CLIHost {
  type: 'node' | 'wsl';
  platform: PlatformInfo;
  exec: Exec;
  ptyExec?: PtyExec;
  env: NodeJS.ProcessEnv;
  cwd: string;
  path: typeof path.posix;
  // Additional platform-specific methods
}
```

### Feature Extensions

Features can extend container functionality through:
- Custom installation scripts
- Configuration modifications
- Lifecycle command hooks
- Environment variable injection

## Data Persistence

### Cache Management

The CLI implements caching for:
- Feature downloads and installations
- User environment probe results
- Configuration parsing results
- Docker image layers

### State Management

- Container identification and tracking
- Workspace state persistence
- User preferences and configuration
- Incremental build optimization

## Implementation Guidelines

### Code Organization

1. **Separation of Concerns**: Clear module boundaries
2. **Async/Await Pattern**: Consistent async handling
3. **Error Propagation**: Proper error handling and bubbling
4. **Logging Integration**: Comprehensive logging throughout
5. **Testing Coverage**: Unit and integration test coverage

### Key Design Patterns

1. **Command Pattern**: Command handlers with options parsing
2. **Strategy Pattern**: Different container creation strategies
3. **Factory Pattern**: Container and configuration factories
4. **Observer Pattern**: Event-driven lifecycle management
5. **Template Method**: Common workflow with customization points

### Dependencies Management

**Core Dependencies**:
- `yargs`: Command-line argument parsing
- `jsonc-parser`: JSON with comments parsing
- `tar`: Archive handling for features
- `semver`: Semantic version handling
- `chalk`: Terminal color output
- `vscode-uri`: URI handling and path resolution

**Build Dependencies**:
- `typescript`: TypeScript compilation
- `esbuild`: Fast JavaScript bundling
- `mocha`: Test framework
- `eslint`: Code linting

## Template System

### Template Architecture

Templates are pre-configured development environment setups that can be applied to projects.

#### Template Definition Structure
```typescript
interface Template {
  // Metadata
  id: string;
  version?: string;
  name?: string;
  description?: string;
  documentationURL?: string;
  licenseURL?: string;
  
  // Template properties
  type?: string;             // Added programmatically during packaging
  fileCount?: number;        // Added programmatically during packaging
  featureIds?: string[];     // Features included in template
  options?: Record<string, TemplateOption>;
  platforms?: string[];      // Supported platforms
  publisher?: string;        // Template publisher
  keywords?: string[];       // Search keywords
  optionalPaths?: string[];  // Optional file paths
  files: string[];           // File list (added during packaging)
}
```

#### Template Option Types
```typescript
type TemplateOption = {
  type: 'boolean';
  default?: boolean;
  description?: string;
} | {
  type: 'string';
  enum?: string[];
  default?: string;
  description?: string;
} | {
  type: 'string';
  default?: string;
  proposals?: string[];
  description?: string;
};
```

### Template Operations

#### Template Application Process
1. **Template Resolution**: Resolve template identifier to specific version
2. **Option Processing**: Process template options and user inputs
3. **File Processing**: Apply template files with variable substitution
4. **Configuration Merge**: Merge template devcontainer.json with existing
5. **Feature Integration**: Install template-specified features
6. **Validation**: Validate resulting configuration

#### Template Publishing
1. **Package Creation**: Bundle template files and metadata
2. **Validation**: Validate template structure and metadata
3. **OCI Packaging**: Create OCI-compatible package
4. **Registry Upload**: Push to OCI registry with proper metadata

## Advanced Configuration Features

### Workspace Management

The CLI provides sophisticated workspace handling through the `Workspace` interface:

```typescript
interface Workspace {
  readonly isWorkspaceFile: boolean;
  readonly workspaceOrFolderPath: string;
  readonly rootFolderPath: string;
  readonly configFolderPath: string;
}
```

**Workspace Resolution**:
- Detects VS Code workspace files (`.code-workspace`)
- Handles both folder and workspace file scenarios
- Manages configuration file discovery
- Supports Git root mounting

### Variable Substitution Engine

The CLI implements a comprehensive variable substitution system supporting:

#### Substitution Variables
```typescript
interface SubstitutionContext {
  platform: NodeJS.Platform;
  configFile?: URI;
  localWorkspaceFolder?: string;
  containerWorkspaceFolder?: string;
  env: NodeJS.ProcessEnv;
}
```

**Supported Variable Types**:
- `${localWorkspaceFolder}` - Local workspace folder path
- `${containerWorkspaceFolder}` - Container workspace folder path
- `${localEnv:VARIABLE_NAME}` - Local environment variable
- `${containerEnv:VARIABLE_NAME}` - Container environment variable
- `${devcontainerId}` - Generated container identifier

**Substitution Phases**:
1. **Pre-container**: Local environment and workspace context
2. **Container**: Container environment and properties
3. **Runtime**: Dynamic values during execution

#### Variable Resolution Process
```typescript
const VARIABLE_REGEXP = /\$\{(.*?)\}/g;

function resolveString(replace: Replace, value: string): string {
  return value.replace(VARIABLE_REGEXP, evaluateSingleVariable.bind(undefined, replace));
}
```

### Mount and Volume Management

#### Mount Types
```typescript
interface Mount {
  type: 'bind' | 'volume' | 'tmpfs';
  source: string;
  target: string;
  external?: boolean;
  consistency?: 'consistent' | 'cached' | 'delegated';
  bind?: {
    propagation?: 'rprivate' | 'private' | 'rshared' | 'shared' | 'rslave' | 'slave';
  };
  volume?: {
    nocopy?: boolean;
  };
  tmpfs?: {
    size?: string;
  };
}
```

#### Mount Processing
1. **Parse Mount Specifications**: Process mount strings and objects
2. **Path Resolution**: Resolve relative paths and variables
3. **Platform Adaptation**: Handle platform-specific path formats
4. **Docker Integration**: Convert to Docker-compatible mount arguments

## OCI Registry Integration

### Feature and Template Distribution

The CLI integrates with OCI (Open Container Initiative) registries for distributing features and templates:

#### OCI Package Structure
```typescript
interface OCIManifest {
  schemaVersion: number;
  mediaType: string;
  config: {
    mediaType: string;
    size: number;
    digest: string;
  };
  layers: Array<{
    mediaType: string;
    size: number;
    digest: string;
  }>;
}
```

#### Registry Operations
1. **Authentication**: Handle registry authentication and tokens
2. **Manifest Operations**: Fetch and validate manifests
3. **Layer Download**: Download and extract package layers
4. **Caching**: Implement local caching for performance
5. **Version Resolution**: Resolve semantic version constraints

### Package Management

#### Feature Packaging
1. **Metadata Collection**: Extract feature metadata from `devcontainer-feature.json`
2. **Script Validation**: Validate installation scripts
3. **Dependency Analysis**: Analyze feature dependencies
4. **Archive Creation**: Create compressed archive with all assets
5. **OCI Manifest**: Generate OCI-compatible manifest

#### Template Packaging
1. **File Discovery**: Scan template directory for files
2. **Option Processing**: Extract template options and metadata
3. **File Filtering**: Apply include/exclude patterns
4. **Archive Creation**: Bundle template files and metadata
5. **Registry Upload**: Push to OCI registry

## HTTP and Network Handling

### Request Management
```typescript
interface RequestOptions {
  method: 'GET' | 'POST' | 'PUT' | 'DELETE';
  url: string;
  headers?: Record<string, string>;
  data?: Buffer | string;
  timeout?: number;
  followRedirects?: boolean;
}
```

#### Network Features
- **Proxy Support**: Automatic proxy detection and configuration
- **Authentication**: Bearer token and basic authentication
- **Retry Logic**: Configurable retry with exponential backoff
- **Progress Tracking**: Download progress reporting
- **Certificate Handling**: Custom CA certificate support

## Process Management and Shell Integration

### Shell Server Architecture

The CLI implements a shell server for executing commands in containers:

```typescript
interface ShellServer {
  exec(command: string, args: string[], options: ExecOptions): Promise<ExecResult>;
  dispose(): Promise<void>;
}
```

#### Shell Features
- **PTY Support**: Pseudo-terminal allocation for interactive commands
- **Environment Isolation**: Proper environment variable handling
- **Signal Handling**: SIGINT, SIGTERM, and other signal propagation
- **Output Streaming**: Real-time output streaming and buffering

### Process Tree Management

The CLI tracks and manages process trees for proper cleanup:

```typescript
interface Process {
  pid: number;
  ppid: number;
  command: string;
  children: Process[];
}
```

#### Process Operations
1. **Tree Discovery**: Build process tree from system information
2. **Signal Propagation**: Send signals to entire process trees
3. **Cleanup Management**: Ensure proper process cleanup on exit
4. **Resource Monitoring**: Track resource usage and limits

## Dotfiles Integration

### Dotfiles Configuration
```typescript
interface DotfilesConfiguration {
  repository?: string;
  installCommand?: string;
  targetPath?: string;
}
```

#### Dotfiles Installation Process
1. **Repository Cloning**: Clone dotfiles repository to container
2. **Install Script Detection**: Automatically detect install scripts
3. **Command Execution**: Run installation commands in container
4. **Permission Handling**: Manage file permissions and ownership
5. **Error Recovery**: Handle installation failures gracefully

## Performance Optimization

### Caching Strategy

The CLI implements multi-level caching:

#### Cache Types
1. **Feature Cache**: Downloaded feature packages and metadata
2. **Image Cache**: Docker layer caching and intermediate images
3. **Configuration Cache**: Parsed configuration files
4. **Environment Cache**: User environment probe results

#### Cache Management
- **TTL-based Expiration**: Time-to-live based cache invalidation
- **Content-based Validation**: Hash-based cache validation
- **Size Limits**: Configurable cache size limits
- **Cleanup Policies**: Automatic cleanup of stale cache entries

### Build Optimization

#### BuildKit Integration
- **Automatic Detection**: Detect BuildKit availability
- **Multi-stage Builds**: Optimize multi-stage Dockerfile builds
- **Layer Caching**: Efficient layer caching and reuse
- **Parallel Builds**: Parallel feature installation and building

#### Incremental Operations
- **Change Detection**: Detect configuration and file changes
- **Selective Rebuilds**: Rebuild only changed components
- **State Persistence**: Persist build state across sessions

## Testing and Quality Assurance

### Test Architecture

The CLI includes comprehensive testing framework:

#### Test Types
1. **Unit Tests**: Individual component testing with mocks
2. **Integration Tests**: End-to-end workflow testing
3. **CLI Tests**: Command-line interface testing
4. **Feature Tests**: Feature installation and validation
5. **Performance Tests**: Performance and resource usage testing

#### Test Infrastructure
```typescript
interface TestContext {
  tmpDir: string;
  containers: string[];
  images: string[];
  cleanup: (() => Promise<void>)[];
}
```

### Validation Framework

#### Configuration Validation
- **JSON Schema**: Schema-based configuration validation
- **Semantic Validation**: Cross-field validation and constraints
- **Feature Compatibility**: Feature compatibility checking
- **Platform Validation**: Platform-specific requirement validation

#### Runtime Validation
- **Container State**: Container health and state validation
- **Service Availability**: Service and port availability checking
- **File System**: File system permissions and structure validation
- **Network Connectivity**: Network connectivity and DNS resolution

## Monitoring and Observability

### Logging System

The CLI implements structured logging with multiple outputs:

#### Log Structure
```typescript
interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  context?: Record<string, any>;
  error?: Error;
}
```

#### Log Destinations
- **Console Output**: Formatted console output with colors
- **File Logging**: Structured file logging for debugging
- **JSON Logging**: Machine-readable JSON format
- **Stream Logging**: Real-time log streaming

### Metrics and Telemetry

#### Performance Metrics
- **Operation Timing**: Time tracking for major operations
- **Resource Usage**: Memory and CPU usage monitoring
- **Network Metrics**: Download speeds and transfer sizes
- **Error Rates**: Error frequency and categorization

#### Event Tracking
- **Lifecycle Events**: Container lifecycle event tracking
- **User Actions**: Command usage and option selection
- **Feature Usage**: Feature installation and usage patterns
- **Performance Events**: Performance milestone tracking

## Security and Compliance

### Security Model

#### Container Security
- **User Isolation**: Proper user namespace handling
- **Capability Management**: Linux capability management
- **Security Contexts**: SELinux and AppArmor integration
- **Secret Management**: Secure handling of secrets and credentials

#### Registry Security
- **Authentication**: Secure registry authentication
- **Transport Security**: TLS/HTTPS for all communications
- **Content Validation**: Package signature verification
- **Vulnerability Scanning**: Integration with vulnerability scanners

### Compliance Features

#### Audit Logging
- **Command Auditing**: Audit trail of CLI commands
- **Configuration Changes**: Track configuration modifications
- **Security Events**: Log security-relevant events
- **Compliance Reporting**: Generate compliance reports

## Extensibility and Plugin Architecture

### Extension Points

The CLI provides several extension points for customization:

#### Custom CLI Hosts
```typescript
interface CLIHost {
  type: 'node' | 'wsl' | 'custom';
  platform: PlatformInfo;
  exec: Exec;
  ptyExec?: PtyExec;
  env: NodeJS.ProcessEnv;
  cwd: string;
  path: typeof path.posix;
  // Platform-specific extensions
}
```

#### Custom Feature Sources
- **Registry Plugins**: Custom registry implementations
- **Local Sources**: Local feature and template sources
- **Authentication Providers**: Custom authentication mechanisms
- **Content Transformers**: Custom content transformation

### Plugin Development

#### Plugin Interface
```typescript
interface Plugin {
  name: string;
  version: string;
  initialize(context: PluginContext): Promise<void>;
  execute(command: string, args: any[]): Promise<any>;
  dispose(): Promise<void>;
}
```

## Deployment and Distribution

### Distribution Methods

#### npm Package
- **Main Distribution**: Primary distribution via npm registry
- **Versioning**: Semantic versioning and release management
- **Dependencies**: Minimal dependency footprint
- **Platform Support**: Cross-platform compatibility

#### Binary Distribution
- **Single Executable**: Self-contained executable generation
- **Platform Binaries**: Platform-specific binary generation
- **Container Images**: Pre-built container images
- **Package Managers**: Integration with system package managers

### Installation and Setup

#### System Requirements
- **Node.js**: Node.js 16+ runtime requirement
- **Docker**: Docker or Podman container runtime
- **Build Tools**: Platform-specific build tool requirements
- **Network Access**: Internet connectivity for registry access

#### Configuration Files
- **Global Configuration**: System-wide CLI configuration
- **User Configuration**: User-specific settings and preferences
- **Project Configuration**: Project-specific overrides
- **Environment Variables**: Environment-based configuration

## Migration and Compatibility

### Version Compatibility

#### Backward Compatibility
- **Configuration Format**: Support for older configuration formats
- **Feature Compatibility**: Backward compatibility for features
- **API Stability**: Stable CLI interface and behavior
- **Migration Guides**: Documentation for version migration

#### Future Compatibility
- **Extensible Design**: Design for future extensibility
- **Deprecation Policy**: Clear deprecation and removal policies
- **Feature Flags**: Feature flags for experimental functionality
- **Version Detection**: Automatic version detection and adaptation

This comprehensive specification provides the complete architectural and functional definition of the DevContainer CLI, enabling reconstruction in any programming language while maintaining full compatibility with the Development Containers Specification. The specification covers all major components, data structures, workflows, and implementation details necessary for a complete reimplementation.