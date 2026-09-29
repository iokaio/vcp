"""Bounded offset-page response adapted from Anthropic mcp-builder.

Source: reference/python_mcp_server.md, Pagination Implementation.
Licensed under Apache-2.0; see ../LICENSE.txt and ../UPSTREAM.md.
VCP modifications: isolate response construction, reject invalid/nonprogressing
pages, and return a dictionary for the target SDK to serialize.
"""


def paginated_result(items, total, limit=20, offset=0):
    """Describe one validated page; does not fetch or execute a tool.

    The caller must bound service response bytes and validate item content.
    ``total`` is the service's count for the same query/snapshot as ``items``.
    """
    for name, value in (("total", total), ("limit", limit), ("offset", offset)):
        if type(value) is not int:
            raise ValueError(f"{name} must be an integer")
    if total < 0 or offset < 0 or not 1 <= limit <= 100:
        raise ValueError("total and offset must be nonnegative; limit must be 1..100")
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
