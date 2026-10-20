# Specifications

Permanent, current specifications for behavior shipped by this crate. A spec is
kept true as the code changes; when they disagree, the disagreement is a defect.

| Number | Feature | Status |
| --- | --- | --- |
| 001 | Deep data | Active |

## Coverage Order

1. File-format surfaces read or written by this crate.
2. Public API surfaces that are documented in the crate root.
3. Behavior relied on by the OpenEXR reference implementation.

## Definition Of Covered

A contract row is `Covered` only when it cites both:

- source evidence: the exact file and symbol implementing it, and
- executable evidence: the exact test name and command, or manual QA steps.

Docs, checkboxes, and plans are not evidence.

## Active Feature Pointer

`specs/001-deep-data/spec.md`. The machine-readable pointer lives in
`.specify/feature.json`.
