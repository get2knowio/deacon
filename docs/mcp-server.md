# MCP Server for deacon (design draft)

This note tracks design details for adding an MCP-compliant server that exposes deacon CLI commands as tools.

See the GitHub issue template in `.github/ISSUE_TEMPLATE/feature-mcp-server.md` for acceptance criteria.

## Goals
- Provide stdio transport first; optional host/port for ws/sse transport
- Map CLI-SPEC.md behaviors to MCP tools with parity
- Stream progress and redact secrets consistently

## Subcommand
- `deacon mcp-server --host <HOST> --port <PORT> --transport <stdio|ws|sse>`
  - host default: `localhost`
  - transport default: `stdio`

## SDK options (evaluate)
- `mcp-protocol-sdk` (docs.rs)
- `mcp-sdk-rs` (docs.rs)

## Tool mapping (initial)
- deacon.readConfiguration
- deacon.build
- deacon.up
- deacon.exec
- deacon.down
- deacon.features.*
- deacon.templates.*
- deacon.config.substitute
- deacon.doctor

## Error mapping
- Map `deacon_core::errors` to MCP error structures; keep codes and messages actionable.

## Progress & logs
- Use tracing spans: config.resolve, container.create, feature.install, lifecycle.run
- Stream progress through MCP content events; keep JSON outputs as structured content

## Tests
- Unit tests for arg mapping and error translation
- Integration: spawn stdio server and call 1-2 tools via a mock JSON-RPC client

## Security
- Loopback bind by default; document risks when exposing network transports
- Enforce redaction settings and never leak secrets in logs

## References
- MCP Spec: https://modelcontextprotocol.io/specification
- Transports: https://modelcontextprotocol.io/docs/concepts/transports
- SDKs: https://docs.rs/mcp-protocol-sdk, https://docs.rs/mcp-sdk-rs
- Examples: https://github.com/modelcontextprotocol/servers
