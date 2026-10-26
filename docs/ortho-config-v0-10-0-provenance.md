# OrthoConfig v0.10.0 documentation provenance

The imported documentation is copied byte-for-byte from the
`leynos/ortho-config` repository. The source was fetched from tag `v0.10.0` and
read from the immutable commit below.

| Source                                   | Tag object                                 | Peeled commit                              |
| ---------------------------------------- | ------------------------------------------ | ------------------------------------------ |
| `https://github.com/leynos/ortho-config` | `996e5a1c9f22ace4f55bf5dcc196d2c0524262bf` | `38812c563e3953a490e3efc202609f37e21a0517` |

The fetched tag object and peeled commit match the values recorded in issue
[#669](https://github.com/leynos/mxd/issues/669). No other ref was used. The
source commit is available at
[`38812c563e3953a490e3efc202609f37e21a0517`](https://github.com/leynos/ortho-config/tree/38812c563e3953a490e3efc202609f37e21a0517).

## Imported files

| Upstream path                     | Destination path                               | SHA-256                                                            |
| --------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------ |
| `docs/users-guide.md`             | `docs/ortho-config-users-guide.md`             | `84d1c97b77024224dc9220a12d4b749205d9bb2db54849185da21c969dda5575` |
| `docs/v0-10-0-migration-guide.md` | `docs/ortho-config-v0-10-0-migration-guide.md` | `b5331aa6cfcc07b3dcd188c620db98d0a1a1f13b0c8ef96d88913d701bca25bf` |
| `docs/v0-9-0-migration-guide.md`  | `docs/ortho-config-v0-9-0-migration-guide.md`  | `c174c424f91c055201d6a8ec644b0a3b2c7aa681ffbd052d9d6152d78eb2f49d` |
| `docs/v0-7-0-migration-guide.md`  | `docs/ortho-config-v0-7-0-migration-guide.md`  | `d5692d45a784a5e148f6a4244eae46a649068ee26422309e2c9095daaabd106d` |
| `docs/v0-6-0-migration-guide.md`  | `docs/ortho-config-v0-6-0-migration-guide.md`  | `6fb6172f34aa85601ba3279e39e6c2af016a64d6360026a435d99143ac25be3e` |

The earlier local users' guide at `docs/ortho-config-users-guide.md` was
replaced. Its previous SHA-256 was
`51ebb55f13c3b8cc49e851dccf40090eb61d0eb42de84c8adc460061b6919507`. Earlier
imported migration guides remain separate historical records.

## Relative-link mapping

The imported files retain their upstream link text and anchors unchanged.
Intra-document anchors continue to resolve in each copy. Cross-document links
inside the upstream `docs/` directory use the following mapping; the source
copies deliberately keep the original relative targets.

| Upstream target                   | Local guide                                                                           |
| --------------------------------- | ------------------------------------------------------------------------------------- |
| `docs/users-guide.md`             | [OrthoConfig user's guide](ortho-config-users-guide.md), preserving the source anchor |
| `docs/v0-10-0-migration-guide.md` | [v0.10.0 migration guide](ortho-config-v0-10-0-migration-guide.md)                    |
| `docs/v0-9-0-migration-guide.md`  | [v0.9.0 migration guide](ortho-config-v0-9-0-migration-guide.md)                      |

The original source names are retained in the copied files, so a relative
`users-guide.md` link in an imported migration guide would otherwise open mxd's
guide at `docs/users-guide.md`. Since the imported files must remain byte-for-
byte copies, Markdown rendering does not rewrite those relative links. For
local navigation, use the following anchor mapping to the OrthoConfig user's
guide instead:

| Imported link fragment                                 | Local destination                                                                                                                      |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| `#use-the-generated-message-identifiers`               | [Use the generated message identifiers](ortho-config-users-guide.md#use-the-generated-message-identifiers)                             |
| `#export-identifier-artefacts-for-tooling`             | [Export identifier artefacts for tooling](ortho-config-users-guide.md#export-identifier-artefacts-for-tooling)                         |
| `#switch-a-boolean-off-from-the-command-line`          | [Switch a boolean off from the command line](ortho-config-users-guide.md#switch-a-boolean-off-from-the-command-line)                   |
| `#switch-between-named-profiles`                       | [Switch between named profiles](ortho-config-users-guide.md#switch-between-named-profiles)                                             |
| `#declare-behaviour-metadata-for-agents`               | [Declare behaviour metadata for agents](ortho-config-users-guide.md#declare-behaviour-metadata-for-agents)                             |
| `#cargo-external-subcommand-entry-points`              | [Cargo external subcommand entry points](ortho-config-users-guide.md#cargo-external-subcommand-entry-points)                           |
| `#choose-the-file-discovery-source`                    | [Choose the file-discovery source](ortho-config-users-guide.md#choose-the-file-discovery-source)                                       |
| `#agent-native-policy-checking`                        | [Agent-native policy checking](ortho-config-users-guide.md#agent-native-policy-checking)                                               |
| `#control-which-configuration-files-become-layers`     | [Control which configuration files become layers](ortho-config-users-guide.md#control-which-configuration-files-become-layers)         |
| `#resolve-discovery-and-merging-without-process-state` | [Resolve discovery and merging without process state](ortho-config-users-guide.md#resolve-discovery-and-merging-without-process-state) |

All other relative targets in the imported copies resolve against the same
immutable upstream commit. Use this URL base for files:

```text
https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/
```

The `examples/hello_world/` directory uses the matching `tree` URL. Imported
source citations retain the upstream `【F:path†Lstart-Lend】` marker verbatim;
resolve one by appending `path#Lstart-Lend` to the URL base above. The
referenced relative paths are `CHANGELOG.md`, `README.md`,
`examples/hello_world/`, `ortho_l10n/README.md`,
`docs/adrs/adr-0004-cargo-external-subcommand-entry-point.md`,
`docs/cargo-orthohelp-api-guide.md`, `docs/cargo-orthohelp-design.md`,
`ortho_config/Cargo.toml`, `ortho_config/src/lib.rs`,
`ortho_config/src/declarative.rs`, `ortho_config/src/discovery/mod.rs`,
`ortho_config/src/error.rs`, `ortho_config/src/file/mod.rs`,
`ortho_config/src/localizer/clap_error.rs`, `ortho_config/src/localizer/mod.rs`,
`ortho_config/src/merge.rs`, `ortho_config/src/post_merge.rs`,
`ortho_config/src/result_ext.rs`, `ortho_config/src/subcommand/selected.rs`,
`ortho_config_macros/Cargo.toml`, `ortho_config_macros/src/derive/load_impl.rs`,
`examples/hello_world/Cargo.toml`,
`examples/hello_world/src/cli/config_loading.rs`,
`examples/hello_world/src/cli/discovery.rs`,
`examples/hello_world/src/cli/mod.rs`,
`examples/hello_world/src/cli/tests/overrides.rs`, and
`docs/adrs/adr-0002-replace-cucumber-with-rstest-bdd.md`.

## Version boundary and history

The imported user's guide and migration guide describe published v0.10.0. The
user's guide explicitly marks per-field localized defaults as unavailable and
command-vocabulary linting as planned; neither is treated as a v0.10.0 API. The
v0.10.0 guide and upstream changelog were read before dependency or test
changes.

The v0.10.0 guide describes the `AgentContext.profiles` retyping and the
`DocMetadata.profiles` addition as changes to struct-literal callers. The
serialized form remains unchanged when profiles are not enabled:
`DocMetadata.profiles` is omitted when `None`, and the derive emits `Some` only
for profile-enabled structs. Profile-enabled metadata therefore adds the
`profiles` field; its defaulted deserialization keeps earlier JSON readable.
The verbatim guide's sentence about serialized form should be read with this
scope.

The upstream changelog's v0.9.0 section states that it also records changes
first published in v0.8.0 and earlier. The changelog has no separate earlier
release sections. The imported v0.6.0 and v0.7.0 guides, together with the
v0.9.0 and v0.10.0 guides, provide the intervening migration notes required for
this v0.3.0 starting point.
