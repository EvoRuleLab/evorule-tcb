# Copyright Assignment Policy

**Effective date**: 2026-07-02
**Initial copyright holder**: DAMU ZHENG <evorulelab@gmail.com>
**Project**: evorule-tcb (https://github.com/EvoRuleLab/evorule-tcb)

This document describes the legal mechanism by which the
copyright (and, where applicable, trademark) in evorule-tcb may
be transferred from the initial individual copyright holder
(DAMU ZHENG) to a successor entity, and the implications of such
a transfer for licensees and contributors.

## 1. Scope

This policy covers:

- The copyright in the evorule-tcb source code, documentation,
  tests, and ancillary files committed to the project repository.
- The trademarks associated with evorule-tcb, which include
  the Chinese transliteration Yuanze and the English word EvoRule as
  textual components (see TRADEMARK.md).
- The role of "Licensor" in any `COMMERCIAL_LICENSE.md` agreement
  signed with commercial licensees.

It does NOT cover:

- Third-party dependencies consumed by evorule-tcb (governed by
  their own licenses).
- Trademarks of other parties.
- Personal copyright of contributors in works not contributed to
  the project.

## 2. Initial State (as of v0.1.0)

- **Copyright holder**: DAMU ZHENG (individual).
- **Trademark holder**: DAMU ZHENG (individual).
- **Commercial Licensor**: DAMU ZHENG (individual).
- **Contribution mechanism**: No formal CLA was used prior to
  v0.1.0. All code was authored by the copyright holder.

## 3. Mechanism of Assignment

### 3.1 Copyright Assignment

Copyright is transferred from the assignor (current copyright
holder) to the assignee (successor entity) by a signed
**Copyright Assignment Agreement (CAA)**. The CAA is a private
legal document; it is not committed to the public repository.

A typical CAA covers:

- **Parties**: assignor (individual) and assignee (legal entity),
  identified by full legal name, address, and (for the entity)
  registration number.
- **Subject matter**: all copyrighted works contributed to the
  evorule-tcb project before the effective date, including code,
  documentation, tests, and configuration files.
- **Grant**: present, absolute, and unconditional assignment of
  all right, title, and interest in the subject matter, including
  all moral rights to the extent waivable under applicable law.
- **Effective date**: the date of execution by both parties.
- **Consideration**: nominal, or as otherwise agreed (e.g. share
  issuance, employment, founder agreement).
- **Warranties**: assignor warrants sole ownership and the right
  to make the assignment; assignee warrants good faith acceptance.
- **Governing law**: jurisdiction agreed by both parties.

This is a legal contract and must be drafted or reviewed by a
lawyer qualified in the relevant jurisdiction. The summary above
is informational only and is not a substitute for legal advice.

### 3.2 Trademark Assignment

Trademarks are transferred by a separate **Trademark Assignment
Agreement** and, in most jurisdictions, must be recorded with the
relevant trademark office (e.g. CNIPA in China, USPTO in the
United States, EUIPO in the European Union). Trademark
recordation is administrative and does not affect the validity
of the assignment between the parties, but it is required to
enforce the mark against third parties.

Trademark assignment typically takes 6 to 12 months to record.
During the recordation period, the project continues to operate
under the previous owner's name, and the new owner's rights vest
from the date of the assignment agreement.

### 3.3 Public Notice

When an assignment is executed:

- A filled-in copy of `LICENSE_CHANGE_NOTICE.md` is committed to
  the repository, recording the change.
- A `CHANGELOG.md` entry is added under a "Changed" section.
- A GitHub release is published summarizing the change and
  linking to the updated `NOTICE` file.
- Known commercial licensees are notified by direct email.
- The `NOTICE`, `COMMERCIAL_LICENSE.md`, and `TRADEMARK.md` files
  are updated to reflect the new holders.

## 4. Effect on Existing Licenses

### 4.1 Open Source License (AGPL-3.0)

The open-source license is **not affected** by the assignment.
The text of the AGPL-3.0 license (in the `LICENSE` file) is a
standard FSF-published text and is not specific to any copyright
holder. Recipients of evorule-tcb under AGPL-3.0 continue to have
exactly the same rights and obligations after the assignment as
they had before.

### 4.2 Commercial License

The licensor named in a `COMMERCIAL_LICENSE.md` agreement
becomes, from the effective date of the assignment, the
successor entity. Existing commercial licensees are offered the
option to:

- **Continue under the original agreement**, with the licensor
  automatically substituted by operation of law or by an
  assignment-of-agreement side letter, OR
- **Sign a re-issued agreement** with the new licensor, at no
  additional cost, to make the substitution explicit.

For a project at v0.1.0 with no existing commercial licensees,
no such transition is required.

## 5. Effect on Contributors

The Contributor License Agreement (`CLA-individual.md`) is drafted
to be **assignment-of-future-friendly**: every grant in the CLA
runs in favor of "the Project and any successor entity". This
means:

- Contributors who signed the CLA before the assignment are
  automatically treated as having granted rights to the
  successor entity, without needing to re-sign.
- Contributors who sign the CLA after the assignment grant
  rights directly to the successor entity.

The successor entity is therefore in a position equivalent to
the original individual copyright holder with respect to all
contributions, past and future.

## 6. Effect on Trademark Users

Trademark users (see `TRADEMARK.md`) continue to use the Marks
under the same policy after the assignment, but the relevant
contact for permission requests becomes the successor entity.
The change in contact details is reflected in an updated
`TRADEMARK.md` and in the public notice.

## 7. Versioning of This Policy

This policy may be amended by the copyright holder at any time.
Material changes (e.g. introduction of a new transfer mechanism
or change in the contributor grant structure) will be announced
in `CHANGELOG.md` and reflected in a new `LICENSE_CHANGE_NOTICE.md`.

## 8. Contact

For questions about this policy, or to initiate an assignment
process, contact:

DAMU ZHENG <evorulelab@gmail.com>

## 9. Disclaimer

This document is informational. It does not constitute legal
advice and does not create a binding commitment to assign
copyright on any specific terms. Any actual assignment must be
documented in a signed Copyright Assignment Agreement drafted
or reviewed by qualified counsel.
