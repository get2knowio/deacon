#!/usr/bin/env python3
"""Fetch a pinned real-world devcontainer corpus into /tmp/realworld-corpus.

This materializes reproducible workspace snapshots from public GitHub repos
without vendoring any third-party content into the repository. Each entry is a
workspace root.

The Tier-1 parity drivers were ported from Python to Rust nextest binaries and
deleted (018-harden-parity-harness): the runners are now
`crates/deacon/tests/parity_corpus_tier1.rs` /
`parity_corpus_merged.rs`, which discover cases under the in-repo corpus root
`fixtures/parity-corpus/` (see `registry.json`) and run under
`cargo nextest run --profile parity` / `make test-parity`. This helper remains an
exploratory aid: to exercise a fetched snapshot, copy it as a case directory
under the corpus root (raising the corpus `min_cases` if you want the floor to
rise) and run `make test-parity`.

The script uses pinned commit SHAs and GitHub's contents API via `gh api`.
"""

from __future__ import annotations

import argparse
import base64
import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from functools import lru_cache
from pathlib import Path
from typing import Iterable


DEFAULT_DEST = Path("/tmp/realworld-corpus")


@dataclass(frozen=True)
class CorpusEntry:
    name: str
    repo: str
    ref: str
    workspace_root: str = ""
    include_paths: tuple[str, ...] = ()
    config_path: str = ""
    notes: str = ""


ENTRIES: tuple[CorpusEntry, ...] = (
    CorpusEntry(
        name="images-javascript-node",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/javascript-node",
        notes="Dockerfile + feature-heavy image recipe with scripts.",
    ),
    CorpusEntry(
        name="images-python",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/python",
        notes="Dockerfile build with multiple official features.",
    ),
    CorpusEntry(
        name="images-go",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/go",
        notes="Dockerfile build plus Go/Node/common-utils features.",
    ),
    CorpusEntry(
        name="images-rust",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/rust",
        notes="Dockerfile build with Rust feature and lifecycle customizations.",
    ),
    CorpusEntry(
        name="images-java",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/java",
        notes="Dockerfile build with Java/Node features.",
    ),
    CorpusEntry(
        name="images-php",
        repo="devcontainers/images",
        ref="31b61b521d55926d62c748b659f24ae71774c0e3",
        workspace_root="src/php",
        notes="Dockerfile build with a checked-in local feature.",
    ),
    CorpusEntry(
        name="templates-javascript-node-postgres",
        repo="devcontainers/templates",
        ref="95f7406a57fc5f0798964a5853c5ac04added322",
        workspace_root="src/javascript-node-postgres",
        notes="Compose-based template workspace with app + Postgres services.",
    ),
    CorpusEntry(
        name="templates-go-postgres",
        repo="devcontainers/templates",
        ref="95f7406a57fc5f0798964a5853c5ac04added322",
        workspace_root="src/go-postgres",
        notes="Compose-based template workspace for Go + Postgres.",
    ),
    CorpusEntry(
        name="try-node",
        repo="microsoft/vscode-remote-try-node",
        ref="45f5e33e47f4b113804ea808b7ce4c90a6823867",
        include_paths=(".devcontainer", "package.json", "package-lock.json", "server.js"),
        notes="Small image-based Node workspace used by the reference ecosystem.",
    ),
    CorpusEntry(
        name="try-python",
        repo="microsoft/vscode-remote-try-python",
        ref="e351212b72f76fb557c6f31956eb44756300b8b4",
        include_paths=(".devcontainer", "app.py", "requirements.txt", "static"),
        notes="Small image-based Python workspace with app files.",
    ),
    CorpusEntry(
        name="try-go",
        repo="microsoft/vscode-remote-try-go",
        ref="f4575309350c5ca2f1495c28a13b4b07088e8cea",
        include_paths=(".devcontainer", "go.mod", "hello", "server.go"),
        notes="Small image-based Go workspace with module files.",
    ),
    CorpusEntry(
        name="try-rust",
        repo="microsoft/vscode-remote-try-rust",
        ref="d3cb2a9843af67d20491c9fde829b2c77230847b",
        include_paths=(".devcontainer", "Cargo.toml", "Cargo.lock", "src"),
        notes="Small image-based Rust workspace with crate sources.",
    ),
    CorpusEntry(
        name="try-java",
        repo="microsoft/vscode-remote-try-java",
        ref="4638a925031f05cca946b8ce6ab640c433c93585",
        include_paths=(".devcontainer", "pom.xml", "src"),
        notes="Small image-based Java workspace with Maven sources.",
    ),
    CorpusEntry(
        name="try-dotnetcore",
        repo="microsoft/vscode-remote-try-dotnetcore",
        ref="ca7ad4d9216a1bcc1469e4ca3545a66ff3e771a0",
        include_paths=(
            ".devcontainer",
            "Program.cs",
            "vscode-remote-try-dotnet.csproj",
            "appsettings.json",
            "appsettings.Development.json",
            "appsettings.HttpsDevelopment.json",
        ),
        notes="Small image-based .NET workspace.",
    ),
    CorpusEntry(
        name="try-cpp",
        repo="microsoft/vscode-remote-try-cpp",
        ref="22af031095a03864b8c2032b6498ff6d21e46c36",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based C++ sample; .devcontainer contains required support files.",
    ),
    CorpusEntry(
        name="try-php",
        repo="microsoft/vscode-remote-try-php",
        ref="9c4c759e95499bb57be2a35e2e0c55f292036908",
        include_paths=(".devcontainer", "index.php"),
        notes="Small image-based PHP workspace.",
    ),
    CorpusEntry(
        name="oss-ruff",
        repo="astral-sh/ruff",
        ref="82b550741e8766224f23ce6d71fea262b866966b",
        include_paths=(".devcontainer",),
        notes="Real OSS Rust/Python workspace with volume mounts and a post-create script.",
    ),
    CorpusEntry(
        name="oss-gh-cli",
        repo="cli/cli",
        ref="57b9b207d900114092f30d78020c193b40621dfa",
        include_paths=(".devcontainer",),
        notes="Real OSS Go workspace with sshd feature and explicit remoteUser.",
    ),
    CorpusEntry(
        name="oss-vscode",
        repo="microsoft/vscode",
        ref="af752dba42ade664df7d6e01f4f459e0c1718512",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace with desktop-lite and rust features.",
    ),
    CorpusEntry(
        name="oss-typescript",
        repo="microsoft/TypeScript",
        ref="7964e22f2b85f16e520f0e902c7fd7b6f0c15416",
        include_paths=(".devcontainer",),
        notes="Image-based workspace with feature metadata and rich VS Code customizations.",
    ),
    CorpusEntry(
        name="oss-fluentui",
        repo="microsoft/fluentui",
        ref="de337cf86b501d2aa7d9b12c472489c7c88d6b24",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace with build args and feature metadata.",
    ),
    CorpusEntry(
        name="oss-promptflow",
        repo="microsoft/promptflow",
        ref="3928a727b406e66d64ff42621534bb58e0ca18ce",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace with remoteEnv, runArgs, and Azure CLI feature.",
    ),
    CorpusEntry(
        name="oss-autogen",
        repo="microsoft/autogen",
        ref="027ecf0a379bcc1d09956d46d12d44a3ad9cee14",
        include_paths=(".devcontainer",),
        notes="Compose-based real repo with many features and explicit workspaceFolder.",
    ),
    CorpusEntry(
        name="oss-fhir-server",
        repo="microsoft/fhir-server",
        ref="7769ecc5eb170920f384f309afa6e852e7fdee78",
        include_paths=(".devcontainer",),
        notes="Compose-based real repo with custom workspaceFolder and helper scripts.",
    ),
    CorpusEntry(
        name="oss-monaco-editor",
        repo="microsoft/monaco-editor",
        ref="7374dcb41a787a63d5885a5be5e6bbc2e6bc338c",
        include_paths=(".devcontainer",),
        notes="Simple image-based workspace with lightweight customizations.",
    ),
    CorpusEntry(
        name="oss-web-dev-for-beginners",
        repo="microsoft/Web-Dev-For-Beginners",
        ref="5f220217d35499881cfff61a5b4c2dab033ab228",
        include_paths=(".devcontainer",),
        notes="Universal-image workspace with a feature and editor customizations.",
    ),
    CorpusEntry(
        name="oss-generative-ai-for-beginners",
        repo="microsoft/generative-ai-for-beginners",
        ref="61a1240c5de4109ceac54142934411365c67c759",
        include_paths=(".devcontainer",),
        notes="Universal-image workspace with hostRequirements and post-create script.",
    ),
    CorpusEntry(
        name="oss-ml-for-beginners",
        repo="microsoft/ML-For-Beginners",
        ref="24028ae995117b45fabb883cf42114e721ed65b5",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace with context '..' and init runArgs.",
    ),
    CorpusEntry(
        name="oss-code-with-engineering-playbook",
        repo="microsoft/code-with-engineering-playbook",
        ref="016770e43d8a75be87b98c000c049f07c4a6e6f8",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace with parent build context.",
    ),
    CorpusEntry(
        name="oss-procdump-linux",
        repo="microsoft/ProcDump-for-Linux",
        ref="f2717626a2960f8e0d0aebd9efe2e95bfe6c43b1",
        include_paths=(".devcontainer",),
        notes="Dockerfile-based workspace selecting a non-default Dockerfile variant.",
    ),
    CorpusEntry(
        name="oss-sample-app-aoai-chatgpt",
        repo="microsoft/sample-app-aoai-chatGPT",
        ref="54f0af2c09bfe71f93da4acfb06422e11f984d71",
        include_paths=(".devcontainer",),
        notes="Image-based workspace with multiple OCI features including latest azd.",
    ),
    CorpusEntry(
        name="oss-agentic-cookbook",
        repo="microsoft/AgenticCookBook",
        ref="32c6b754cff666962b6cd4679a2bdd9183fbe28e",
        include_paths=(".devcontainer",),
        config_path=".devcontainer/.devcontainer.json",
        notes="Nonstandard config path (.devcontainer/.devcontainer.json) with hostRequirements.",
    ),
    CorpusEntry(
        name="oss-presidio-anonymizer",
        repo="microsoft/presidio",
        ref="83ab7eb85609c49d9b0b17c44b5c025575966876",
        include_paths=(".devcontainer/presidio-anonymizer", "presidio-anonymizer/Dockerfile.dev"),
        config_path=".devcontainer/presidio-anonymizer/devcontainer.json",
        notes="Nested config path with workspaceMount and cross-directory Dockerfile/context references.",
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dest", type=Path, default=DEFAULT_DEST, help="Corpus output directory.")
    parser.add_argument(
        "--name",
        dest="names",
        action="append",
        default=[],
        help="Fetch only the named entry (repeatable).",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Remove the destination directory before fetching.",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List the pinned corpus entries and exit.",
    )
    return parser.parse_args()


def run_gh_json(endpoint: str) -> object:
    result = subprocess.run(
        ["gh", "api", endpoint],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


@lru_cache(maxsize=None)
def get_commit_tree_sha(repo: str, ref: str) -> str:
    commit = run_gh_json(f"repos/{repo}/commits/{ref}")
    return commit["commit"]["tree"]["sha"]


@lru_cache(maxsize=None)
def get_tree_modes(repo: str, ref: str) -> dict[str, str]:
    tree_sha = get_commit_tree_sha(repo, ref)
    tree = run_gh_json(f"repos/{repo}/git/trees/{tree_sha}?recursive=1")
    return {item["path"]: item.get("mode", "") for item in tree.get("tree", [])}


@lru_cache(maxsize=None)
def get_content_metadata(repo: str, ref: str, remote_path: str) -> object:
    endpoint = f"repos/{repo}/contents"
    if remote_path:
        endpoint = f"{endpoint}/{remote_path}"
    return run_gh_json(f"{endpoint}?ref={ref}")


def get_file_bytes(repo: str, ref: str, remote_path: str) -> bytes:
    metadata = get_content_metadata(repo, ref, remote_path)
    if not isinstance(metadata, dict) or metadata.get("type") != "file":
        raise RuntimeError(f"Expected file at {repo}:{remote_path}@{ref}")

    content = metadata.get("content")
    if not isinstance(content, str):
        raise RuntimeError(f"Missing inline file content for {repo}:{remote_path}@{ref}")
    return base64.b64decode(content)


def write_file(repo: str, ref: str, remote_path: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(get_file_bytes(repo, ref, remote_path))

    mode = get_tree_modes(repo, ref).get(remote_path, "")
    if mode == "100755":
        destination.chmod(0o755)


def fetch_tree(repo: str, ref: str, remote_dir: str, destination: Path) -> None:
    metadata = get_content_metadata(repo, ref, remote_dir)
    if not isinstance(metadata, list):
        raise RuntimeError(f"Expected directory at {repo}:{remote_dir}@{ref}")

    destination.mkdir(parents=True, exist_ok=True)
    for child in metadata:
        child_type = child["type"]
        child_name = child["name"]
        child_remote_path = child["path"]
        child_destination = destination / child_name
        if child_type == "dir":
            fetch_tree(repo, ref, child_remote_path, child_destination)
        elif child_type == "file":
            write_file(repo, ref, child_remote_path, child_destination)
        else:
            raise RuntimeError(
                f"Unsupported GitHub contents type {child_type!r} at {repo}:{child_remote_path}@{ref}"
            )


def fetch_path(repo: str, ref: str, remote_path: str, destination: Path) -> None:
    metadata = get_content_metadata(repo, ref, remote_path)
    if isinstance(metadata, list):
        fetch_tree(repo, ref, remote_path, destination)
        return
    if metadata.get("type") != "file":
        raise RuntimeError(f"Unsupported path type at {repo}:{remote_path}@{ref}: {metadata}")
    write_file(repo, ref, remote_path, destination)


def selected_entries(names: Iterable[str]) -> list[CorpusEntry]:
    if not names:
        return list(ENTRIES)

    by_name = {entry.name: entry for entry in ENTRIES}
    missing = sorted(set(names) - set(by_name))
    if missing:
        raise SystemExit(f"Unknown corpus entries: {', '.join(missing)}")
    return [by_name[name] for name in names]


def write_manifest(entries: list[CorpusEntry], dest: Path) -> None:
    manifest_path = dest / "_manifest.json"
    payload = {
        "entries": [asdict(entry) for entry in entries],
        "generated_by": "fixtures/parity-corpus/fetch_realworld_corpus.py",
    }
    manifest_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def list_entries(entries: list[CorpusEntry]) -> None:
    for entry in entries:
        payload = {
            "name": entry.name,
            "repo": entry.repo,
            "ref": entry.ref,
            "workspace_root": entry.workspace_root,
            "include_paths": list(entry.include_paths),
            "config_path": entry.config_path,
            "notes": entry.notes,
        }
        print(json.dumps(payload, sort_keys=True))


def fetch_entry(entry: CorpusEntry, dest: Path) -> None:
    target = dest / entry.name
    if target.exists():
        shutil.rmtree(target)
    target.mkdir(parents=True, exist_ok=True)

    if entry.workspace_root:
        fetch_tree(entry.repo, entry.ref, entry.workspace_root, target)

    for include_path in entry.include_paths:
        fetch_path(entry.repo, entry.ref, include_path, target / include_path)


def main() -> None:
    args = parse_args()
    entries = selected_entries(args.names)

    if args.list:
        list_entries(entries)
        return

    if args.clean and args.dest.exists():
        shutil.rmtree(args.dest)
    args.dest.mkdir(parents=True, exist_ok=True)

    for entry in entries:
        print(f"==> fetching {entry.name} from {entry.repo}@{entry.ref}")
        fetch_entry(entry, args.dest)

    write_manifest(entries, args.dest)
    print(f"\nFetched {len(entries)} workspace snapshots into {args.dest}")


if __name__ == "__main__":
    main()
