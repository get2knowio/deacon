# Cache Reuse Example

## What This Demonstrates

This example shows how the DevContainer feature system uses:
- **Digest-based caching** to identify identical features across runs
- **Cache key computation** using feature content and configuration
- **Cache hit/miss detection** in logs
- **Performance optimization** by avoiding redundant installations

## Feature Setup

This example includes a simple cacheable feature:

### Cached Feature
- A feature that can be cached and reused
- Same feature content across multiple installations
- Located in `./cached-feature/`

## How Feature Caching Works

The caching system uses content-based addressing:

1. **Cache Key Generation**:
   - Compute digest (SHA256) of feature content (metadata + install.sh)
   - Include configuration options in the cache key
   - Result: unique identifier for this exact feature state

2. **Cache Lookup**:
   - First run: Cache miss → install feature → store result
   - Second run: Cache hit → reuse previous installation

3. **Cache Storage**:
   - Multi-level cache (memory + disk)
   - TTL (Time To Live) for cache entries
   - LRU eviction when cache is full

## DevContainer Specification References

- **[Feature Caching](https://containers.dev/implementors/spec/#caching)**: How features are cached
- **[Content Addressing](https://containers.dev/implementors/spec/#feature-resolution)**: Using digests for cache keys
- Up SPEC: Performance and caching: ../../../docs/subcommand-specs/up/SPEC.md#11-performance-considerations

## Commands

### First Run (Cache Miss)

Run the configuration to install features:
```sh
# First time - will install and cache
deacon read-configuration --config devcontainer.json
```

Look for log messages indicating cache operations:
- `Cache miss for feature: cached-feature`
- `Installing feature: cached-feature`
- `Cached feature result for: cached-feature`

### Second Run (Cache Hit)

Run the same configuration again:
```sh
# Second time - will use cache
deacon read-configuration --config devcontainer.json
```

Look for log messages indicating cache hit:
- `Cache hit for feature: cached-feature`
- `Reusing cached installation for: cached-feature`

### Viewing Cache Statistics

The cache system tracks statistics:
```json
{
  "hits": 5,
  "misses": 3,
  "entries": 10,
  "memory_usage_bytes": 1024000,
  "hit_rate": 0.625
}
```

### Cache Inspection

To see cache details with debug logging:
```sh
RUST_LOG=debug deacon read-configuration --config devcontainer.json
```

Debug logs show:
- Cache key computation
- Cache lookup operations
- Cache storage operations
- Cache statistics updates

## Cache Invalidation

The cache is automatically invalidated when:
- Feature content changes (new digest)
- Feature configuration/options change
- TTL expires (configurable timeout)
- Cache is manually cleared

## Why This Matters

Feature caching provides significant benefits:
- **Performance**: Avoid redundant work across multiple environments
- **Consistency**: Same feature content = same cached result
- **Offline capability**: Cached features work without network access
- **Resource efficiency**: Reduce bandwidth and computation

## Cache Location

The cache is typically stored at:
- Memory cache: In-process (fast, temporary)
- Disk cache: `~/.deacon/cache/features/` (persistent)

## Comparing Cache Behavior

### Run 1: Fresh Installation
```
Time: 5 seconds
Cache: Miss
Installation: Full
Storage: Cache entry created
```

### Run 2: Cached Installation
```
Time: 0.1 seconds
Cache: Hit
Installation: Skipped
Storage: Cache entry reused
```

The dramatic time difference demonstrates the cache effectiveness.

## Cache Key Components

A feature cache key includes:
- Feature ID
- Feature version
- Feature source (local path, OCI reference, etc.)
- Feature content digest (SHA256 of all files)
- Configuration options (user-provided values)

Changing ANY of these components creates a new cache key (cache miss).

## Offline Operation

This example is fully offline - the feature is local (no registry required). The cache works identically for both local and registry-based features.

Copy the entire directory and run commands without network access to see caching in action.
