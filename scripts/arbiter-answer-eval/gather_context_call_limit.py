# SPDX-License-Identifier: Apache-2.0
"""External tool-call budgets for Gather Context's hosted diagnostics."""

import common


LEGACY_TOOL_CALL_LIMIT = 12
DEFAULT_TOOL_CALL_LIMIT = 24
TOOL_CALL_LIMITS = (LEGACY_TOOL_CALL_LIMIT, DEFAULT_TOOL_CALL_LIMIT)
_PROMPT_BUDGET = "- You may make at most {} tool calls. Stop as soon as you have what you need; fewer calls are better."


def validate_tool_call_limit(value: int) -> int:
    common.require(type(value) is int and value in TOOL_CALL_LIMITS,
                   "Gather Context diagnostics require an external tool-call limit of 12 or 24")
    return value


def set_prompt_tool_call_limit(prompt: str, limit: int) -> str:
    limit = validate_tool_call_limit(limit)
    original = _PROMPT_BUDGET.format(LEGACY_TOOL_CALL_LIMIT)
    common.require(prompt.count(original) == 1, "Shared actor prompt tool-call instruction changed")
    if limit == LEGACY_TOOL_CALL_LIMIT:
        return prompt
    return prompt.replace(original, _PROMPT_BUDGET.format(limit), 1)


def reply_artifact(ordinal: int, limit: int) -> str | None:
    """An over-limit refusal has a call audit row and uses no reply file."""
    if not 1 <= ordinal <= limit:
        return None
    name = f"reply-{ordinal:03d}.json"
    return name if ordinal <= LEGACY_TOOL_CALL_LIMIT else f"work/{name}"
