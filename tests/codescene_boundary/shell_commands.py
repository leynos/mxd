"""Read the commands a workflow `run:` block executes.

A requirement that a lane *runs* a command cannot match the command as a
substring of the script: `echo make test-codescene-boundary` and a comment
naming it both contain it and run nothing. So a script is split into simple
commands the way the shell would, and a command counts only when its words
begin one of them.

This reader is for requirements ("the lane runs X"). The prohibitions in this
contract stay substring tests, because there over-matching is the safe
direction and this reader deliberately under-matches. It models lists (`;`,
`&&`, `||`, `&`), pipelines, subshell parentheses, line continuations,
comments and leading variable assignments; it does not model functions,
`eval`, or command substitution, and a line it cannot tokenize contributes no
command rather than a guessed one. The shape follows whitaker's reader of the
same name.
"""

from __future__ import annotations

import itertools
import re
import shlex
import typing as typ

# The characters shlex groups into operator tokens.
_OPERATOR_CHARACTERS: typ.Final = ";&|()"

# A leading `NAME=value` word sets the environment of the command after it
# rather than being the command.
_ASSIGNMENT: typ.Final = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


def _is_operator(token: str) -> bool:
    """Whether a token separates one simple command from the next."""
    return bool(token) and set(token) <= set(_OPERATOR_CHARACTERS)


def _line_tokens(line: str) -> list[str]:
    """One line's words and operators, stopping at a comment."""
    lexer = shlex.shlex(line, posix=True, punctuation_chars=_OPERATOR_CHARACTERS)
    lexer.whitespace_split = True
    # shlex would end a word at a `#` anywhere; the shell starts a comment only
    # at the start of a word, so comments are cut here instead.
    lexer.commenters = ""
    try:
        tokens = list(lexer)
    except ValueError:
        # An unbalanced quote: the line is read as no command at all, which
        # under-matches, the safe direction for a requirement.
        return []
    cut = next(
        (index for index, token in enumerate(tokens) if token.startswith("#")),
        len(tokens),
    )
    return tokens[:cut]


def command_segments(script: str) -> list[list[str]]:
    """Each simple command in a script, as its words.

    >>> command_segments("set -e; FOO=1 make check  # why")
    [['set', '-e'], ['make', 'check']]
    """
    joined = script.replace("\\\n", " ")
    return [
        command
        for line in joined.splitlines()
        for is_operator, words in itertools.groupby(
            _line_tokens(line), key=_is_operator
        )
        if not is_operator
        if (command := list(itertools.dropwhile(_ASSIGNMENT.match, words)))
    ]


def runs_command(script: str, command: str) -> bool:
    """Whether a script executes a command, not merely mentions it.

    >>> runs_command("echo make check", "make check")
    False
    """
    words = command.split()
    return any(segment[: len(words)] == words for segment in command_segments(script))
