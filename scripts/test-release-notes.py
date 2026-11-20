"""Exercise the PR check and the Markdown output used for a release."""

import unittest
from pathlib import Path
from subprocess import run
from tempfile import TemporaryDirectory


command = Path(__file__).resolve().with_name("release-notes.py")
heading = "## Release notes\n\n"
note = "- Added PGN export for games\nwith comments and variations."


class ReleaseNotesTest(unittest.TestCase):
    def test_pr_description(self):
        example = "```md\n" + heading + "- Example only.\n```\n\n"
        template = (command.parents[1] / ".github/pull_request_template.md").read_text()
        for body, status in [
            (example + heading + note, 0),
            (heading + "- N/A", 0),
            (template, 1),
            (example, 1),
            (heading, 1),
            (heading + "- N/A\n" + note, 1),
            (heading + note + "\n## Verification\n- Ran the CLI.", 1),
        ]:
            with self.subTest(body=body):
                result = run([command, "--check"], input=body, text=True, capture_output=True)
                self.assertEqual(result.returncode, status, result.stderr)

    def test_tagged_releases(self):
        with TemporaryDirectory() as repository:
            git = [
                "git", "-C", repository,
                "-c", "user.name=Test", "-c", "user.email=test@example.org",
                "-c", "commit.gpgSign=false", "-c", "tag.gpgSign=false",
                "-c", "core.hooksPath=/dev/null",
            ]
            run([*git, "init", "-q"], check=True)
            for subject, body, tag in [
                ("feat: read games", "Before release notes", "first"),
                ("chore: update CI", heading + "- N/A", "internal"),
                ("feat: export games", heading + note, "second"),
            ]:
                run([*git, "commit", "-q", "--allow-empty", "-m", subject, "-m", body], check=True)
                run([*git, "tag", tag], check=True)

            first = run([command, "first"], cwd=repository, text=True, capture_output=True, check=True)
            self.assertEqual(first.stdout, "")
            self.assertIn("feat: read games", first.stderr)
            later = run([command, "first", "second"], cwd=repository, text=True, capture_output=True, check=True)
            self.assertEqual(later.stdout, note + "\n")
            self.assertEqual(later.stderr, "")


if __name__ == "__main__":
    unittest.main()
