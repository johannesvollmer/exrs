# Requirements Checklist: Deep Data

## Spec Quality

- [x] User stories are independently testable.
- [x] Acceptance criteria are observable.
- [x] Non-goals are explicit.
- [x] Risks are named, including the Weta fixture trap.

## Contract Quality

- [x] Every important behavior has a contract row.
- [x] Every row is `Covered`, `Partial`, or `Open`.
- [x] `Covered` rows cite source evidence.
- [x] `Covered` rows cite test or manual QA evidence.
- [x] Required evidence is listed before work starts.

## Implementation Readiness

- [x] Tasks are small enough for single commits.
- [x] Persistence and migration behavior is documented.
- [x] Shared logic ownership is named (`block::deep_layout`, `compression`).

## Open Items

- [x] T018/T019 OpenEXR cross-reads.
- [x] T024 duplicated fixture removal.
- [x] T025 callback block-index validation.
- [x] T027 full feature matrix.
- [x] T028/T029 PR body and commits.
