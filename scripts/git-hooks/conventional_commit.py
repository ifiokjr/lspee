#!/usr/bin/env python3
"""Validate that a commit message follows the Conventional Commits format."""

import re
import sys

# type(scope): description
pattern = r"^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z-]+\))?: .+"


def main() -> int:
    commit_msg_file = sys.argv[1] if len(sys.argv) > 1 else None
    if not commit_msg_file:
        return 0
    with open(commit_msg_file) as f:
        msg = f.read()
    if re.match(pattern, msg):
        return 0
    print("Error: Commit message does not follow Conventional Commits format.")
    print("Expected format: type(scope): description")
    print("Valid types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert")
    print("Example: feat(daemon): add memory eviction support")
    return 1


if __name__ == "__main__":
    sys.exit(main())
