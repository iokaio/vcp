# SPDX-License-Identifier: Apache-2.0
"""Bounded offset-page response adapted from Anthropic mcp-builder.

Source: reference/python_mcp_server.md, Pagination Implementation.
Licensed under Apache-2.0; see ../LICENSE.txt and ../UPSTREAM.md.
VCP modifications: isolate response construction, reject invalid/nonprogressing
pages, make the maximum page size a validated parameter, and return a
dictionary for the target SDK to serialize.
"""

DEFAULT_MAX_LIMIT = 100


def paginated_result(items, total, limit=20, offset=0, max_limit=DEFAULT_MAX_LIMIT):
    """Describe one validated page; does not fetch or execute a tool.

    The caller must bound service response bytes and validate item content.
    ``total`` is the service's count for the same query/snapshot as ``items``.
    ``max_limit`` is the server's page-size ceiling; ``limit`` must be
    ``1..max_limit``. Keep it equal to the maximum advertised in the tool schema.
    """
    values = (("total", total), ("limit", limit), ("offset", offset), ("max_limit", max_limit))
    for name, value in values:
        if type(value) is not int:
            raise ValueError(f"{name} must be an integer")
    if max_limit < 1:
        raise ValueError("max_limit must be at least 1")
    if total < 0 or offset < 0 or not 1 <= limit <= max_limit:
        raise ValueError(
            f"total and offset must be nonnegative; limit must be 1..{max_limit}"
        )
    if not isinstance(items, list) or len(items) > limit:
        raise ValueError("items must be a list no larger than the requested limit")
    count = len(items)
    if count and offset + count > total:
        raise ValueError("page contents exceed the reported total")
    has_more = total > offset + count
    if has_more and count == 0:
        raise ValueError("empty page cannot advance toward the reported total")
    return {
        "total": total,
        "count": count,
        "offset": offset,
        "items": items,
        "has_more": has_more,
        "next_offset": offset + count if has_more else None,
    }
