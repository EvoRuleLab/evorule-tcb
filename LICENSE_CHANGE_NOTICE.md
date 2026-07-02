# License Change Notice

**Template version**: 2026-07-02

This is a **template** for documenting future license changes in
the evorule-tcb project. When the license terms, copyright holder,
or trademark holder changes, a copy of this template is filled in
and placed in the repository, with the change recorded in
`CHANGELOG.md` and announced via a GitHub release.

---

## Change Content

**Effective date**: YYYY-MM-DD

This release changes the following legal attributes of the
evorule-tcb project:

- **Copyright holder**: (changed from `<OLD>` to `<NEW>` /
  unchanged)
- **License terms**: (unchanged / changed from `<OLD>` to `<NEW>`)
- **Trademark holder**: (changed from `<OLD>` to `<NEW>` /
  unchanged)
- **Licensor for commercial agreements**: (changed from `<OLD>` to
  `<NEW>` / unchanged)

A summary of the change and its implications for existing
licensees is provided below.

### What Stays the Same

- The open-source license text (AGPL-3.0) remains in `LICENSE`
  unless explicitly changed above.
- The dual-licensing model (AGPL-3.0 OR commercial) remains the
  default licensing policy of the project.
- Existing contributions made before the effective date are
  governed by the Contributor License Agreement (`CLA-individual.md`
  or `CLA-entity.md`), which contains a "successor entity" clause
  and is therefore not affected by the change in copyright holder.

### What Changes

- The "Copyright" line in `NOTICE` is updated to reflect the new
  copyright holder.
- `COMMERCIAL_LICENSE.md` is re-issued with the new licensor
  named in the preamble.
- `TRADEMARK.md` is updated to identify the new trademark holder.
- The `COPYRIGHT_ASSIGNMENT_POLICY.md` may be updated to reflect
  the new structural relationship between the project and its
  legal owner.

## Reason

A short paragraph explaining the business, legal, or organizational
reason for the change. For example:

- "The project has been assigned from its initial individual
  copyright holder to a successor corporate entity, in order to
  support long-term sustainability, employee equity, and
  separation of personal and corporate liability. See
  `COPYRIGHT_ASSIGNMENT_POLICY.md` for the assignment mechanism."
- "The license has been upgraded from AGPL-3.0 to AGPL-3.0-or-later
  to align with ecosystem conventions. No existing licensee is
  affected adversely, and the dual-license model is preserved."

## Historical Code Explanation

A paragraph clarifying the legal status of code committed before
the effective date. For a copyright assignment, this typically
reads:

"All code, documentation, and other copyrighted works committed
to the evorule-tcb repository before the effective date above is
covered by the Copyright Assignment Agreement signed on YYYY-MM-DD
between the previous copyright holder and the new copyright holder.
No relicensing of historical code occurs as a result of this
change. Contributions made under the previous copyright holder
are, by the terms of the Contributor License Agreement
(`CLA-individual.md`), granted to 'the project and any successor
entity', and therefore the grant is automatically effective for
the new copyright holder from the date of the assignment."

## Related Repositories

A list of other repositories, packages, or distribution channels
that share the same copyright and license structure, so that
downstream users and contributors can understand the full scope
of the change.

Examples:

- evorule-tcb (this repository)
- evorule-tcb-cli
- evorule-tcb-benchmarks
- evorule-tcb-sdks/python
- evorule-tcb-sdks/rust

If a related repository is NOT affected by the change (for example,
because it has its own copyright holder), state that explicitly.

---

## Usage Notes

When filling in this template:

1. Replace all `<placeholder>` values with the actual values.
2. Delete the section "What Changes" sub-bullets that do not apply.
3. Add the filled-in notice to the repository root (replacing any
   previous version of this file).
4. Append an entry to `CHANGELOG.md` under a new section, e.g.
   `### Changed - YYYY-MM-DD`.
5. Publish a GitHub release that includes this file in the
   release notes.
6. Notify any known commercial licensees of the change.
