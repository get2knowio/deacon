#!/usr/bin/env python3

"""
Polls a Projects v2 board for items in a specific Status and moves them to another Status.

This script searches the given project (ORG/PROJECT_NUMBER) for items belonging to the
current repository whose Status option matches STATUS_READY. For each matching item,
it will:

1. Create a Copilot agent task for the issue.
2. Optionally post a kickoff comment to start work.
3. Update the item's Status to STATUS_INFLIGHT after successful task creation.

Environment variables required:

  ORG               – GitHub organization login
  PROJECT_NUMBER    – Projects v2 board number
  STATUS_READY      – Name of the Status option that indicates an item is ready
  STATUS_INFLIGHT   – Name of the Status option to set when work begins
  GH_TOKEN          – GitHub token with repo and projects write permissions

Optional environment variables:

  COPILOT_KICKOFF   – Comment body to post when starting work

Note: This script relies on the gh CLI being available in the PATH.
"""

import json
import re
from urllib.parse import urlparse
import os
import subprocess
import shutil
from typing import Any, Dict, List, Optional, Tuple
from urllib import request, error


def gh_graphql(query: str, **vars: Any) -> str:
    """Call the GitHub GraphQL API directly and return raw JSON string.

    Removes dependency on an external shell helper and avoids shell quoting
    issues. Requires GH_PROJECTS_TOKEN or GH_TOKEN to be set in the environment.
    """
    token = os.environ.get("GH_PROJECTS_TOKEN") or os.environ.get("GH_TOKEN")
    if not token:
        raise SystemExit("::error::GH_PROJECTS_TOKEN or GH_TOKEN not set")

    # Coerce common numeric variables to int to satisfy GraphQL types (e.g., Int!)
    variables: Dict[str, Any] = {}
    for k, v in vars.items():
        if k.lower() == "number" and isinstance(v, str) and v.isdigit():
            variables[k] = int(v)
        else:
            variables[k] = v

    body = json.dumps({"query": query, "variables": variables}).encode("utf-8")
    req = request.Request(
        url="https://api.github.com/graphql",
        data=body,
        headers={
            "Authorization": f"bearer {token}",
            "Accept": "application/json",
            # Required for Projects (v2) GraphQL access
            "GraphQL-Features": "projects_next_graphql",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with request.urlopen(req) as resp:
            return resp.read().decode("utf-8")
    except error.HTTPError as e:
        details = e.read().decode("utf-8", errors="ignore") if hasattr(e, "read") else str(e)
        raise SystemExit(f"::error::GraphQL HTTP {e.code}: {details}")


def gh_command(*args: str) -> None:
    """Execute a gh CLI command."""
    subprocess.check_call(["gh", *args])


def normalize(text: str) -> str:
    """Normalize a status name by removing emoji/punctuation and lowering case."""
    return "".join(ch for ch in text if ch.isalnum() or ch.isspace()).lower()


def fetch_all_items(project_id: str) -> List[Dict[str, Any]]:
    """Fetch all ProjectV2 items via pagination (100 per page).

    Returns a list of item nodes with the fields required by the poller.
    """
    items: List[Dict[str, Any]] = []
    after: Optional[str] = None
    while True:
        q = """
        query($id:ID!,$after:String){
            node(id:$id){
                ... on ProjectV2{
                    items(first:100, after:$after){
                        nodes{
                            id
                            content{
                                __typename
                                ... on Issue {
                                    id
                                    number
                                    repository { name owner { login } }
                                }
                                ... on PullRequest {
                                    id
                                    number
                                    repository { name owner { login } }
                                }
                            }
                            fieldValues(first:50){
                                nodes{
                                    ... on ProjectV2ItemFieldSingleSelectValue {
                                        field { ... on ProjectV2SingleSelectField { id name } }
                                        name
                                        optionId
                                    }
                                }
                            }
                        }
                        pageInfo { hasNextPage endCursor }
                    }
                }
            }
        }
        """
        resp = json.loads(gh_graphql(q, id=project_id, after=after))
        if resp.get("errors"):
            msgs = "; ".join(e.get("message", "") for e in resp["errors"]) or "unknown GraphQL error"
            raise SystemExit(f"::error::GraphQL error while listing items: {msgs}")
        node = (resp.get("data") or {}).get("node") or {}
        items_conn = node.get("items") or {}
        items.extend(items_conn.get("nodes") or [])
        page = items_conn.get("pageInfo") or {}
        if not page.get("hasNextPage"):
            break
        after = page.get("endCursor")
        if not after:
            break
    return items


def main() -> None:
    org = os.environ["ORG"]
    project_number = os.environ["PROJECT_NUMBER"]
    status_ready = os.environ["STATUS_READY"]
    status_inflight = os.environ["STATUS_INFLIGHT"]
    repo_env = os.environ.get("GITHUB_REPOSITORY")
    if not repo_env:
        raise SystemExit("::error::GITHUB_REPOSITORY not set")
    owner, repo = repo_env.split("/")

    kickoff = os.environ.get("COPILOT_KICKOFF", "").strip()

    # Preflight: verify token and Projects v2 access early for clearer errors
    preflight_check(org, project_number)

    # Fetch project details (id and fields). Items will be fetched with pagination.
    query = """
    query($org:String!,$number:Int!){
        organization(login:$org){
            projectV2(number:$number){
                id
                fields(first:50){
                    nodes{
                        ... on ProjectV2SingleSelectField { id name options{ id name } }
                        ... on ProjectV2Field { id name }
                    }
                }
            }
        }
    }
    """
    data = json.loads(gh_graphql(query, org=org, number=project_number))
    # Surface GraphQL errors early with a clear message
    if data.get("errors"):
        msgs = "; ".join(e.get("message", "") for e in data["errors"]) or "unknown GraphQL error"
        raise SystemExit(f"::error::GraphQL error while loading project: {msgs}")

    org_data = (data.get("data") or {}).get("organization") or {}
    project = org_data.get("projectV2")
    if not project:
        # Provide a helpful hint by listing visible org projects and their numbers
        try:
            list_q = """
            query($org:String!){
              organization(login:$org){
                projectsV2(first:50){
                  nodes{ number title closed }
                }
              }
            }
            """
            listing = json.loads(gh_graphql(list_q, org=org))
            nodes = (
                ((listing.get("data") or {}).get("organization") or {}).get("projectsV2") or {}
            ).get("nodes") or []
            if nodes:
                visible = ", ".join(
                    f"{n.get('number')} - {n.get('title')}{' (closed)' if n.get('closed') else ''}"
                    for n in nodes if n
                )
                print(f"Visible org projects for '{org}': {visible}")
        except Exception:
            # Best-effort hint; ignore failures here
            pass
        raise SystemExit(
            "::error::Project not found or not accessible. Verify ORG and PROJECT_NUMBER, and ensure GH_TOKEN (PAT) has org Projects access and the user is a member with visibility to this project."
        )

    fields_conn = project.get("fields")
    if fields_conn is None:
        raise SystemExit(
            "::error::Project fields are not accessible. Token may lack Projects v2 read access."
        )

    # Locate the Status field (single select) and resolve option IDs using normalization
    status_nodes = (fields_conn.get("nodes") or [])
    status_field = next(
        n for n in status_nodes if n and n.get("name") == "Status" and n.get("options") is not None
    )
    # Locate optional PR field to store PR id (create as NUMBER if missing)
    pr_field = None
    for fld in status_nodes:
        if not fld or not fld.get("name"):
            continue
        if normalize(fld["name"]) == "pr":
            pr_field = fld["id"]
            break
    if not pr_field:
        create_field_mut = """
        mutation($project:ID!,$name:String!){
          createProjectV2Field(input:{ projectId:$project, dataType: NUMBER, name:$name }){
            projectV2Field{ id name }
          }
        }
        """
        created = json.loads(gh_graphql(create_field_mut, project=project["id"], name="PR"))
        pr_field = (
            created.get("data", {})
            .get("createProjectV2Field", {})
            .get("projectV2Field", {})
            .get("id")
        )
        if not pr_field:
            print("Warning: failed to create PR field; will skip storing PR id")

    want_ready = normalize(status_ready)
    want_inflight = normalize(status_inflight)
    # Additional statuses that should gate intake (only one active at a time)
    gate_status_names = [
        "In Flight",
        "Debrief",
        "Remediation",
        "Verification",
        "Ready for Integration",
    ]
    want_gate = [normalize(s) for s in gate_status_names]
    opt_ready = None
    opt_inflight = None
    opt_gate: List[str] = []
    for option in status_field["options"]:
        key = normalize(option["name"])
        if key == want_ready or all(tok in key for tok in want_ready.split()):
            opt_ready = option["id"]
        if key == want_inflight or all(tok in key for tok in want_inflight.split()):
            opt_inflight = option["id"]
        if any(key == g or all(tok in key for tok in g.split()) for g in want_gate):
            opt_gate.append(option["id"])
    if not opt_ready or not opt_inflight:
        raise SystemExit("::error::Could not resolve Status option IDs. Check Status option names.")

    # Fetch all items with pagination (100 per page)
    items = fetch_all_items(project_id=project["id"])

    # Gate: If ANY item on the project is in one of the gate statuses, do nothing
    for item in items:
        for fv in item["fieldValues"]["nodes"] or []:
            if fv.get("optionId") in opt_gate:
                print(
                    "Active item already in progress (one of: In Flight, Debrief, Remediation, Verification, Ready for Integration). Skipping intake."
                )
                return

    # Find candidate ISSUES belonging to this repository with Status == Ready
    candidates: List[Tuple[str, int]] = []  # (item_id, issue_number)
    for item in items:
        content = item["content"]
        if not content:
            continue
        # Only handle Issues (ignore PRs)
        if content.get("__typename") != "Issue":
            continue
        repo_info = content["repository"]
        if repo_info["owner"]["login"] != owner or repo_info["name"] != repo:
            continue
        # Check each field value for Status
        for fv in item["fieldValues"]["nodes"] or []:
            if fv.get("optionId") == opt_ready:
                candidates.append((item["id"], content["number"]))
                break

    if not candidates:
        print("No items in Ready for Takeoff for this repository.")
        return

    # Select exactly one candidate to process (choose the lowest issue number for determinism)
    item_id, number = min(candidates, key=lambda x: x[1])

    print(f"Processing Issue #{number}")

    started = False
    session_url: Optional[str] = None
    try:
        # Build a task description from the issue content
        # Ensure gh CLI is available before attempting agent-task
        if shutil.which("gh") is None:
            raise FileNotFoundError("gh CLI not found on PATH")
        issue_json = subprocess.check_output(
            ["gh", "issue", "view", str(number), "--json", "title,body,url"],
            text=True,
        )
        issue = json.loads(issue_json)
        desc = (
            f"Issue: {issue.get('title', '')}\n"
            f"URL: {issue.get('url', '')}\n\n"
            f"{issue.get('body', '') or ''}\n\n"
            f"Please address this issue and include: Closes #{number}"
        )

        # Create the agent task in the current repository context without following logs
        # to keep this poller responsive.
        print("Creating Copilot agent task...")
        result = subprocess.run(
            ["gh", "agent-task", "create", "-F", "-", "-R", f"{owner}/{repo}"],
            input=desc,
            text=True,
            capture_output=True,
            check=True,
        )
        # Try to extract a URL from stdout
        m = re.search(r"https?://\S+", result.stdout or "")
        if m:
            session_url = m.group(0).strip()
            print(f"✓ Created agent task: {session_url}")
        started = True
    except subprocess.CalledProcessError as e:
        # Fatal - agent task creation is the primary purpose
        print(f"✗ agent-task creation failed with exit code {e.returncode}")
        if e.stdout:
            print(f"stdout: {e.stdout}")
        if e.stderr:
            print(f"stderr: {e.stderr}")
        print("ERROR: Cannot proceed without agent task. Status will remain unchanged.")
    except FileNotFoundError as e:
        print(f"✗ gh CLI not found: {e}")
        print("ERROR: Cannot proceed without gh CLI. Status will remain unchanged.")
    # 2. Post kickoff comment with the session URL if available
    if started and session_url:
        gh_command("issue", "comment", str(number), "--body", f"Assigned to copilot session: {session_url}")

    # 2b. Parse PR id from the session URL and store in project field 'PR' (NUMBER) if present
    if started and session_url and pr_field:
        try:
            parsed = urlparse(session_url)
            parts = [p for p in parsed.path.split("/") if p]
            pr_id_value = None
            for idx, part in enumerate(parts):
                if part == "pull" and idx + 1 < len(parts):
                    candidate = parts[idx + 1]
                    if candidate.isdigit():
                        pr_id_value = candidate
                    break
            if pr_id_value:
                # Prefer setting as NUMBER type; embed number literal to avoid var type issues
                mutation_set_number = """
                mutation($project:ID!,$item:ID!,$field:ID!){
                  updateProjectV2ItemFieldValue(input:{
                    projectId:$project,
                    itemId:$item,
                    fieldId:$field,
                    value:{ number:__PR_NUMBER__ }
                  }){ projectV2Item{ id } }
                }
                """.replace("__PR_NUMBER__", str(int(pr_id_value)))
                try:
                    gh_graphql(
                        mutation_set_number,
                        project=project["id"],
                        item=item_id,
                        field=pr_field,
                    )
                except Exception:
                    # Fallback to text set if number update fails (e.g., preexisting text field)
                    mutation_set_text = """
                    mutation($project:ID!,$item:ID!,$field:ID!,$text:String!){
                      updateProjectV2ItemFieldValue(input:{
                        projectId:$project,
                        itemId:$item,
                        fieldId:$field,
                        value:{ text:$text }
                      }){ projectV2Item{ id } }
                    }
                    """
                    gh_graphql(
                        mutation_set_text,
                        project=project["id"],
                        item=item_id,
                        field=pr_field,
                        text=pr_id_value,
                    )
        except Exception as e:
            # Non-fatal if parsing or field update fails
            print(f"Warning: could not store PR id: {e}")

    # 3. Update Status to In Flight only if we successfully started work
    if started:
        mutation = """
        mutation($project:ID!,$item:ID!,$field:ID!,$option:ID!){
          updateProjectV2ItemFieldValue(input:{
            projectId:$project,
            itemId:$item,
            fieldId:$field,
            value:{ singleSelectOptionId:$option }
          }){ projectV2Item{ id } }
        }
        """
        gh_graphql(
            mutation,
            project=project["id"],
            item=item_id,
            field=status_field["id"],
            option=opt_inflight,
        )
    else:
        print("No action taken (agent task not configured or failed)")


def preflight_check(org: str, project_number: str) -> None:
    """Run quick checks to ensure GH_TOKEN is present and Projects v2 is accessible.

    Emits friendly errors early instead of failing deeper in the workflow.
    """
    # 1) Token present and valid
    try:
        viewer_q = """
        query{ viewer { login } }
        """
        data = json.loads(gh_graphql(viewer_q))
        if not data.get("data", {}).get("viewer", {}).get("login"):
            raise SystemExit("::error::GH_TOKEN appears invalid: no viewer login returned")
    except SystemExit:
        # Re-raise with same message; gh_graphql prints detailed HTTP context
        raise

    # 2) Minimal Projects v2 access (org + project id only)
    try:
        probe = """
        query($org:String!,$number:Int!){
          organization(login:$org){ projectV2(number:$number){ id } }
        }
        """
        resp = json.loads(gh_graphql(probe, org=org, number=project_number))
        if resp.get("errors"):
            msgs = "; ".join(e.get("message", "") for e in resp["errors"]) or "unknown error"
            raise SystemExit(
                f"::error::Token lacks access to organization projects or Projects v2 is disabled: {msgs}"
            )
    except SystemExit:
        raise


if __name__ == "__main__":
    main()
