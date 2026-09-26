# License Notice

Hireling imports structured Pathfinder 2e rules data from the Foundry VTT
community PF2e system packs ([foundryvtt/pf2e](https://github.com/foundryvtt/pf2e),
releases pinned by tag, archive in `licenses/foundry-pf2e/`). This file states
which license covers what, for every source lane the rules corpus can contain.

- **Imported rows** (the Foundry pack import): pack documents declare their
  own license per row (`system.publication.license`). Remaster content is
  licensed under the **ORC** (see the archived `ORCLicense.md`); legacy
  pre-remaster content is licensed under the **OGL 1.0a** (see the archived
  `OpenGameLicense.md`). Both license texts are archived verbatim, with the
  release they were taken from and their sha256s recorded in
  `licenses/foundry-pf2e/SOURCE.md`.
- **Core rows** (future built-in reference rows): reserved — no core rows
  exist yet; any future core row ships with its license stated here.
- **Custom rows** (homebrew created in-app): the party's own work; where a
  custom entry paraphrases Paizo text, it ships under the Paizo Community
  Use Policy, consistent with the ORC notice above. Custom rows are never
  touched by the import.

The Foundry/Paizo partnership permission visible in the upstream README is
the upstream project's, not ours — this product relies on ORC / OGL 1.0a /
CUP only.

## Verdict mechanics

`hireling license-verdict` checks (a) this file exists and covers every lane,
(b) the archived license texts exist and match the sha256s recorded in
`licenses/foundry-pf2e/SOURCE.md`, and (c) every distinct publication license
across imported corpus rows is covered below. **Green** gates public exposure;
**red** means nothing ships public — the private POC deployment is unaffected.

```text
license-coverage:
imported: ORC, OGL 1.0a
core: reserved
custom: CUP, party-owned
```
