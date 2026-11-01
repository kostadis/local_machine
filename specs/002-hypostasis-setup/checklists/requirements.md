# Specification Quality Checklist: `local-machine hypostasis` — Set Up Hypostasis on a New Machine

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-11-01
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Iteration 1: three [NEEDS CLARIFICATION] markers remain (FR-007 authority-file source,
  FR-009 local services, FR-010 data scope). "Scope is clearly bounded" depends on FR-009 and
  FR-010, so it stays unchecked until the owner answers.
- Domain names (hypostasis, `mneme`, DGX, Ubuntu, sudo) are the problem's own vocabulary,
  not implementation choices, and are allowed.
- Iteration 2 (owner answers 2026-11-01: Q1 A, Q2 A, Q3 C): FR-007 now seeds the
  authority file from an owner-kept source and never overwrites it. FR-009 covers rpg-lib
  only, installed and not started. FR-010–FR-010d cover git-hosted data trees (including
  partial checkouts) and restoring the library DB and palaces from a configured backup, only
  when missing, through the owning tool, verified. Edge cases, entities, assumptions, and
  SC-006/SC-007 updated to match. All items pass.
- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
- Iteration 3 (2026-11-01): the owner said "the local machine hypostasis command starts up
  rpg lib". The `local-machine hypostasis` subcommand now has `start`/`stop` for rpg-lib
  (US3, FR-023 to FR-029, FR-018a, SC-009). The address is read from hypostasis's authority
  file (FR-022). No mneme-side dependency remains. All items pass.
- Iteration 4 (2026-11-01): the owner said "i would also like hypostasis to startup flexai
  social and combat". "Local services" is now rpg-lib, flexai-social (5105), and
  flexai-combat (5106).
  - `start`/`stop` manage all three together.
  - FlexAI data files (commercial xlsx) are restored from the backup location.
  - FlexAI addresses and data locations live in this subcommand's config, because they are
    not in hypostasis's authority file.
  - flexai-social's DGX endpoint source is left to the planning config-sharing review.
  - All items pass.
