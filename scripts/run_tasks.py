#!/usr/bin/env python3
"""
Simple task runner for specs/001-docker-compose-runner/tasks.md

Behavior:
- Parse lines with checkbox tasks in the form: "- [ ] T123 Description" or "- [X] T123 ..."
- For each UNCOMPLETED task ("[ ]"), execute a shell command (echo) including the task ID.
- Process tasks sequentially, one at a time.
- For each task, run the command the specified number of times (default 3), re-reading the tasks file
    between attempts to see if the task has been marked complete externally. If after all retries the
    same task is still uncompleted, stop the script.
- Between each task (i.e., after finishing one task and before starting the next), run a separate
    verification command. If the verification command fails (non-zero exit), abort the program
    immediately. The default verify command is "make test-fast".

Notes:
- This script intentionally uses a trivial command (echo). Replace or extend as needed.
- The parser is tolerant of additional tags like [P] [US1] after the ID.
"""
from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Iterable, List


TASK_LINE_RE = re.compile(
    r"^- \[(?P<mark> |x|X)\]\s+(?P<id>T\d+)\b(?P<rest>.*)$"
)


@dataclass
class Task:
    task_id: str
    completed: bool
    raw_line: str


def parse_tasks(file_path: str) -> List[Task]:
    tasks: List[Task] = []
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                m = TASK_LINE_RE.match(line)
                if not m:
                    continue
                mark = m.group("mark").lower()
                task_id = m.group("id")
                tasks.append(Task(task_id=task_id, completed=(mark == "x"), raw_line=line))
    except FileNotFoundError as e:
        raise FileNotFoundError(f"Tasks file not found: {file_path}") from e
    return tasks


def find_uncompleted(tasks: Iterable[Task]) -> List[Task]:
    return [t for t in tasks if not t.completed]


def run_process(task_id: str, attempt: int, file_path: str, command_template: str, use_shell: bool) -> int:
    """Run the external process for a task.

    The command_template can include placeholders:
      - {id}: task id (e.g., T013)
      - {attempt}: current attempt number (1-based)
      - {file}: absolute path to the tasks.md file

    When use_shell=False, the command string is split with shlex.split.
    """
    try:
        rendered = command_template.format(id=task_id, attempt=attempt, file=file_path)
    except Exception as e:
        print(f"ERROR: failed to render command template for {task_id}: {e}", file=sys.stderr)
        return 1

    try:
        if use_shell:
            completed = subprocess.run(rendered, shell=True, check=False)
        else:
            completed = subprocess.run(shlex.split(rendered), check=False)
        return completed.returncode
    except Exception as e:  # Broad except since this is a simple utility
        print(f"ERROR: failed to execute command for {task_id}: {e}", file=sys.stderr)
        return 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Parse tasks.md and run a command per uncompleted task, with retries.")
    parser.add_argument(
        "--file",
        default=os.path.join("specs", "001-docker-compose-runner", "tasks.md"),
        help="Path to tasks.md (default: specs/001-docker-compose-runner/tasks.md)",
    )
    parser.add_argument(
        "--retries",
        type=int,
        default=3,
        metavar="N",
        help="Number of times to repeat the command per task before stopping if still uncompleted (default: 3, must be >= 0)",
    )
    parser.add_argument(
        "--command",
        default="echo running task {id} (attempt {attempt})",
        help=(
            "Command template to execute per task. Placeholders: {id}, {attempt}, {file}. "
            "Default: 'echo running task {id} (attempt {attempt})'"
        ),
    )
    parser.add_argument(
        "--verify",
        dest="verify_command",
        default="make test-fast",
        help=(
            "Verification command to run between tasks; if it fails, the program aborts. "
            "Placeholders supported: {id}, {attempt}, {file}. Default: 'make test-fast'"
        ),
    )
    parser.add_argument(
        "--shell",
        action="store_true",
        help="Execute the command via the shell (useful for pipelines, redirection).",
    )
    parser.add_argument(
        "--sleep",
        type=float,
        default=0.5,
        help="Seconds to sleep between attempts (default: 0.5)",
    )
    args = parser.parse_args(argv)

    # Validate that retries is non-negative
    if args.retries < 0:
        parser.error(f"--retries must be non-negative, got {args.retries}")

    file_path = args.file
    retries = args.retries

    tasks = parse_tasks(file_path)
    uncompleted = find_uncompleted(tasks)

    if not uncompleted:
        print("No uncompleted tasks found. Nothing to do.")
        return 0

    for idx, task in enumerate(uncompleted):
        print(f"Processing {task.task_id} ...")
        for attempt in range(1, retries + 1):
            rc = run_process(task.task_id, attempt, os.path.abspath(file_path), args.command, args.shell)
            if rc != 0:
                print(f"Command failed for {task.task_id} (attempt {attempt}), continuing attempts...", file=sys.stderr)
            # Re-read file to see if this task has been marked complete externally.
            current = parse_tasks(file_path)
            current_map = {t.task_id: t for t in current}
            now = current_map.get(task.task_id)
            if now and now.completed:
                print(f"Task {task.task_id} is now completed. Moving to next.")
                break

            if attempt < retries:
                time.sleep(args.sleep)

        else:
            # If we didn't break out of the loop, task still uncompleted after retries
            print(
                f"Task {task.task_id} remains uncompleted after {retries} attempts. Stopping.",
                file=sys.stderr,
            )
            return 2

        # Run verification command after each task (including the last)
        verify_rc = run_process(
            task_id="VERIFY",
            attempt=1,
            file_path=os.path.abspath(file_path),
            command_template=args.verify_command,
            use_shell=args.shell,
        )
        if verify_rc != 0:
            print(
                f"Verification command failed with exit code {verify_rc}. Aborting.",
                file=sys.stderr,
            )
            return verify_rc

    print("All uncompleted tasks processed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
