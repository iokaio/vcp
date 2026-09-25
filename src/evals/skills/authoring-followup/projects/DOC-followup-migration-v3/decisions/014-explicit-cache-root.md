# Decision 014: Explicit cache location for scheduled exports
Status: accepted, 2026-07-02. Applies starting with Exporter 2.4.

Scheduled-export job definitions must use the absolute cache_root key. The older cache_dir key was resolved relative to the scheduler working directory and is rejected in 2.4. Manual export commands keep their existing --cache-dir flag and behavior. Jobs already using an absolute cache_root require no edit.

The migration changes configuration interpretation only. It does not move existing cache contents. Operators retain their existing cache directories and point cache_root at the intended absolute directory. Do not delete caches as a migration step.

There is no automatic rewrite of job definitions and no relative-path fallback. Preserve this accepted decision as historical evidence.
