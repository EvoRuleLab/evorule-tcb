<!--
  Copyright (c) 2026 DAMU ZHENG <evorulelab@gmail.com>

  This file is part of evorule-tcb.

  evorule-tcb is dual-licensed under the GNU Affero General Public License
  v3.0 (AGPL-3.0) or a commercial license. See LICENSE and DUAL_LICENSE.md
  in the repository root for details.

  SPDX-License-Identifier: AGPL-3.0-or-later
-->

# EvoRule-TCB Migration Guide

**Author**: DAMU ZHENG
**Email**: evorulelab@gmail.com
**Initial version**: v0.1.0 (2026-07-02)
**Last updated**: 2026-07-02

This document tracks breaking changes and migration paths between
evorule-tcb versions. As the project follows [Semantic Versioning](https://semver.org/),
each section corresponds to a major (X.0.0) or minor (x.Y.0) version bump.

---

## Current version: v0.1.0 (Initial Release)

**Release date**: 2026-07-01
**Source**: 23 Rust files in `crates/tcb/src/`, 9 license/legal files at repo root.

### Migration TO v0.1.0 (new installations)

No code change required. Follow the install instructions in README.md.

### Migration FROM v0.1.0

v0.1.0 is the first public release. There is no prior version to migrate from.

---

## Planned migrations

### v0.1.0 to v0.1.1 (planned ~2026-08-02)

**Scope**: NOTICE copyright holder transfer + documentation corrections.
**Code changes**: none planned.
**Action required for downstream users**: none.

Detailed changes will be listed in `CHANGELOG.md` when v0.1.1 ships.

### Future versions

This section will be filled in as new versions are released. The template is:

```
### vX.Y.Z to vX.Y.W (released YYYY-MM-DD)

**Scope**: ...
**Breaking**: yes / no
**Action required**: ...
```

---

## Version compatibility matrix

| From    | To      | Breaking? | Action required | Released    |
|---------|---------|-----------|-----------------|-------------|
| (none)  | v0.1.0  | n/a       | initial release | 2026-07-02  |

---

## How to read this guide

- **MAJOR version bumps** (X.0.0) introduce breaking changes. Every
  downstream user MUST read the corresponding section before upgrading.
- **MINOR version bumps** (x.Y.0) introduce new features in a backward-
  compatible manner. Reading the section is recommended but not required.
- **PATCH version bumps** (x.y.Z) contain only bug fixes. No action required.

---

## License

This document is part of evorule-tcb and is dual-licensed under
AGPL-3.0-or-later or a commercial license. See LICENSE and DUAL_LICENSE.md
in the repository root for details.

**Author**: DAMU ZHENG
**Email**: evorulelab@gmail.com
**Repository**: https://github.com/EvoRuleLab/evorule-tcb
