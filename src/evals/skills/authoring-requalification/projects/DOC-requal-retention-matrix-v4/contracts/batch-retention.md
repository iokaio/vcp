# Retention batch contract

A request may contain at most 24 entries. Each entry's `note` may contain at
most 4,096 Unicode scalar values. The aggregate payload is the UTF-8 encoding of
all note values joined with one LF byte between adjacent values and may contain
at most 16,384 bytes. Scalar values are not UTF-8 bytes, grapheme clusters, or
display cells.

Validate in this order: entry count, each note's scalar count in request order,
then aggregate UTF-8 bytes. The first failing class returns respectively
`RET_ENTRY_COUNT`, `RET_NOTE_SCALARS`, or `RET_PAYLOAD_BYTES`. Validation occurs
before persistence and every rejection is atomic.

The required acceptance matrix is:

| ID | Input | Expected |
|---|---|---|
| E23 | 23 entries, all other limits below maximum | accept |
| E24 | 24 entries, all other limits below maximum | accept |
| E25 | 25 entries | reject `RET_ENTRY_COUNT`, zero writes |
| S4095 | one note with 4,095 Unicode scalars | accept |
| S4096 | one note with 4,096 Unicode scalars | accept |
| S4097 | one note with 4,097 Unicode scalars | reject `RET_NOTE_SCALARS`, zero writes |
| B16383 | aggregate encoded notes plus separators total 16,383 UTF-8 bytes | accept |
| B16384 | aggregate encoded notes plus separators total 16,384 UTF-8 bytes | accept |
| B16385 | aggregate encoded notes plus separators total 16,385 UTF-8 bytes | reject `RET_PAYLOAD_BYTES`, zero writes |
| M25 | 25 entries also exceeding both content limits | reject `RET_ENTRY_COUNT`, zero writes |

At least one byte-boundary fixture uses a four-byte Unicode scalar so a character
count cannot accidentally stand in for the encoded-byte count.
