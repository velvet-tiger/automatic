# Third-Party Notices

This file lists third-party material that Automatic's own source code is adapted from. It does not list package dependencies. Those are recorded in `package-lock.json` and `src-tauri/Cargo.lock`.

## NVIDIA SkillSpector

- Project: <https://github.com/NVIDIA/skillspector>
- Version used: commit `a50b9c9` (2026-10-04)
- Licence: Apache License, Version 2.0
- Copyright: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.

### What is adapted

The regular expressions in `src-tauri/src/core/asset_security_rules.rs` are adapted from SkillSpector's static pattern lists in `src/skillspector/nodes/analyzers/`:

| Automatic finding code | SkillSpector lists |
|---|---|
| `conversation-exfiltration` | P3, E4, P8 |
| `behaviour-manipulation` | P4 |
| `refusal-suppression` | AR1, AR2 |
| `prompt-leakage` | P6, P7 |
| `memory-poisoning` | MP1, MP3 |
| `credential-harvesting` | E2 |
| `file-enumeration` | E3 |
| `remote-code-fetch` | SC2 |
| `obfuscated-execution` | SC3 |

### Changes made

- The patterns were moved from Python to Rust and grouped under Automatic's finding codes.
- SkillSpector's context filtering, confidence scores and severity model were not carried over. Every adapted rule reports a warning.
- Some patterns were removed and others narrowed to reduce false positives. The header of `asset_security_rules.rs` describes how.

### Licence notice

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this material except in compliance with the License. You may obtain a copy of the License at

<http://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations under the License.
