#!/usr/bin/env python3
"""Check a PR description on stdin or collect release notes from squash commits."""

import re
import subprocess
import sys


def release_notes(body):
    lines = body.splitlines()
    fence = ""
    for index, line in enumerate(lines):
        marker = re.fullmatch(r" {0,3}(`{3,}|~{3,})(.*)", line)
        if fence:
            if marker and marker[1].startswith(fence) and not marker[2].strip():
                fence = ""
        elif marker:
            fence = marker[1]
        elif line.rstrip() == "## Release notes":
            break
    else:
        raise ValueError("End the description with a '## Release notes' section.")

    entries = []
    for position, line in enumerate(lines[index + 1:], start=index + 1):
        if not line.strip():
            continue
        if line.startswith("- ") and line[2:].strip():
            entries.append("- " + line[2:].strip())
            continue

        starts_block = re.match(r"\s*(?:#{1,6}(?:\s|$)|>|[-*+]\s|\d+[.)]\s|`{3,}|~{3,}|<!--|[-=]+$)", line)
        wrapped = lines[position - 1].strip() and not starts_block
        if not entries or not (wrapped or line.startswith("  ")):
            raise ValueError("Use a '- ' bullet for each release note, or '- N/A'.")
        entries[-1] += "\n" + line.rstrip()

    if entries == ["- N/A"]:
        return []
    if not entries or any(entry.splitlines()[0] == "- N/A" for entry in entries):
        raise ValueError("Provide at least one release note or a single '- N/A' bullet.")
    return entries


arguments = sys.argv[1:]
if arguments == ["--check"]:
    try:
        release_notes(sys.stdin.read())
    except ValueError as error:
        sys.exit(str(error))
elif len(arguments) in (1, 2):
    try:
        commits = [
            subprocess.check_output(
                ["git", "rev-parse", "--verify", "--end-of-options", revision + "^{commit}"],
                text=True,
            ).strip()
            for revision in arguments
        ]
        history = subprocess.check_output(
            ["git", "log", "--first-parent", "--reverse", "-z", "--format=%h %s%n%b",
             "..".join(commits), "--"],
            text=True,
        )
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)

    for commit in history.split("\0"):
        if commit:
            subject, _, body = commit.partition("\n")
            try:
                entries = release_notes(body)
            except ValueError as error:
                print(f"Review {subject}. {error}", file=sys.stderr)
                continue
            for entry in entries:
                print(entry)
else:
    sys.exit("Usage: release-notes.py --check | [PREVIOUS_TAG] TAG")
