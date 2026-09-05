# Editor memory and highlighting costs

## What happened

Undo retained a complete file copy for every edit. Syntax rendering rebuilt the parser on each text revision and scanned every highlight range for every line. Search results retained complete matching lines and copied them during rendering, even when the text was visually truncated.

## Changes

Undo now stores the replaced range, deleted text and prior cursor, selection and dirty state. All text mutations use the same splice operation. Undo records use internal LF separators; clipboard text retains the file's line endings. Input normalization runs only on new edits, so undo preserves literal carriage returns already present in a file. Edits also clear collapsed selection anchors, preventing the old phantom-selection behavior.

Each editor tab retains its parser and parse tree. A UTF-8-safe edit range updates the parser; a forward sweep assigns ordered highlight ranges to lines. Text revisions still copy the line vector, join the document and query all styles, so this is not constant-time editing. Language changes replace parser state; theme and font changes refresh rendering without discarding the parser.

Search stores at most 200 bytes of preview text per result, centered near the match and aligned to UTF-8 boundaries. Paths and preview strings are shared. Rendering borrows results and derives file counts from contiguous groups. Debounce, cancellation, result order and file/line metadata remain intact.

## Verification

`cargo check -p vu` passed. `cargo test -p vu -p vu-core -p vu-ghostty` passed 318 tests, with the optional highlighting benchmark run separately and passing. Validation used the existing pinned Ghostty build cache. The four changed files pass rustfmt, and the complete diff passes whitespace checks.

An independent static review of undo restoration and search previews found no blocking regressions. The existing behavior of recording an all-carriage-return paste as an edit remains unchanged.

An independent comparison replayed 108,000 editing states against the original buffer, checking text, cursor, selection, dirty state, revisions and undo depth. That comparison clears collapsed selections before mutations because the original implementation can panic after merging a line with a stale collapsed anchor. Separate regressions check that line-join case and literal carriage returns across cut, paste and undo.

Measured cases on the local Mac:

| Case | Before | After |
| --- | --- | --- |
| Additional heap after 64 one-character edits to a 1 MiB, 8,192-line file | 79,175,775 bytes | 6,783 bytes |
| Mapping 60,003 highlight ranges onto 20,001 lines | 1.271 seconds | 6.474 milliseconds |
| Stored text for 200 results from 512 KiB lines ending in a match | 104,857,600 bytes | 7,600 bytes |

The undo measurement verifies complete original text after all 64 undos. The highlighting benchmark asserts identical text runs. The search measurement uses lines within the existing file-size limit and verifies the complete match remains visible. All search preview text together is bounded at 40,000 bytes for 200 results, excluding paths and other metadata.

## Lessons and limits

Apply input normalization before recording edits, never while restoring stored source text. Verify the framework's actual data model in tests: font size belongs to text layout, not syntax-run attributes.

These are isolated measurements, not whole-app speedups. The installed app was not replaced. Interactive typing, scrolling, restart restoration and Windows/Linux execution were not tested in this pass.
