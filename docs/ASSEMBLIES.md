# Assemblies (Ensamblajes)

An **Assembly** is a reusable subtree of scene nodes with an explicit link back
to the file it came from. The product calls it *Assembly* (Spanish:
*Ensamblaje*) because the useful idea is assembling a known set of parts, not
inheriting hidden behavior at runtime.

The command names `prefab.capture` and `prefab.instantiate`, and the file
extension `.prefab.ron`, are legacy identifiers kept for compatibility. New
commands, UI text, and documentation use **Assembly**.

## What an instance stores

Instantiating an Assembly creates ordinary scene nodes. Nothing is live-linked
at runtime: the instance keeps its own copies, transforms, components,
variables, and scripts.

The instance root also stores:

| Field | Meaning |
| --- | --- |
| `linked_assembly.source` | Normalized path below `assets/`, for example `parts/shelf.prefab.ron`. |
| `linked_assembly.version` | Schema version of the source file. Version 1 is the current format. |
| `linked_assembly.source_hash` | Stable FNV-1a 64 fingerprint of the exact serialized source bytes, formatted as `fnv1a64:...`. |
| `source_asset` | The same project-relative source path, kept for existing asset tooling. |
| `source_schema_version` | The source `version`, kept for compatibility with asset metadata. |

The hash identifies the source revision that produced the instance. It is an
informational fingerprint, not a security hash and not an automatic update
trigger: the editor never rewrites a scene because a file changed on disk.

## Authoring workflow

1. Build the subtree in the scene.
2. Capture it as a new source file:

   ```text
   /prefab.capture target=<uuid|path|name> file=parts/shelf.prefab.ron
   ```

   The command creates the file under the project `assets` directory, requires
   the `.prefab.ron` extension, and never overwrites an existing file. Capturing
   clears the source root's own Assembly link so a source cannot point at
   itself.
3. Place the Assembly from the Assets panel, or instantiate it from a command:

   ```text
   /prefab.instantiate file=parts/shelf.prefab.ron parent=<uuid|path|name>
   ```

   Both placement and instantiation validate the source, its nested Assembly
   links, and the scene slot budget before mutating the document. The new root
   is stamped with `source`, `version`, and `source_hash`.

Instances can be moved, renamed, recolored, and edited like normal nodes. Those
edits are instance overrides; the source file is not modified.

## Commands

All three commands accept `target` as a UUID, an explicit path, or an
unambiguous node name. `selected` is accepted as a target; the canonical
command context has no editor selection, so it resolves only when the scene
contains exactly one linked Assembly. Selection-aware callers should pass the
selected root's UUID or path. When `target` is omitted, the same rule applies.

Every action requires a target whose `linked_assembly` points at a
`.prefab.ron` file, and reports a `CommandOutput` error instead of partially
applying changes.

### `assembly.revert`

```text
/assembly.revert target=<uuid|path|name>
```

Reloads the linked source, validates it, and replaces the instance subtree from
the source. The instance root keeps:

- `uuid`
- `name`
- local `position`, `rotation`, and `scale`
- `color`
- `visible` and `locked`
- `variables`

All other root components and all child nodes come from the source, so revert
discards child overrides. The root's Assembly link is refreshed with the
current source `version` and `source_hash`.

Preserved root variables are not retargeted. An `Entity` variable that points
at a child which no longer exists after the refresh becomes a dangling
reference and must be fixed manually.

### `assembly.update`

```text
/assembly.update target=<uuid|path|name>
```

Reloads and validates the source, then replaces the instance's **child
topology** while keeping the instance root unchanged. The root's UUID, name,
transform, color, visibility, lock, variables, and other root components remain
the instance's values, including overrides.

Version 1 deliberately preserves root overrides only and replaces children.
Changes to the source root's primitive, mesh, or other components are not
applied by `assembly.update`; use `assembly.revert` when the root components
must be refreshed. Child-level overrides are lost, and nested Assembly links
present in the source are copied as links, not expanded.

### `assembly.make_unique`

```text
/assembly.make_unique target=<uuid|path|name>
```

Clears `linked_assembly` on the instance root and on descendants whose link
points at the same source. Links to other sources are left untouched, and every
current value is preserved. The command never edits or deletes the source file.
It is the way to detach an instance before making local structural changes that
should not be reverted or updated from the shared source.

All three commands report `changed` through the normal command path, so the
host records one undoable scene edit when the scene actually changes. Their
preview form runs against a disposable scene and never writes project files.

## Validation and cycle rules

Validation before instantiate, revert, or update is split between the editor
and `raf_core`:

- The editor resolves paths below `assets/`, performs all filesystem I/O, and
  enforces the 8 MiB per-file limit.
- `raf_core::scene::prefab::walk_assembly_dependencies` walks nested
  `linked_assembly.source` values through a caller-supplied resolver. It does no
  filesystem I/O itself.

The walk:

- rejects empty, absolute, backslash, drive-letter, and `..`-traversal sources;
- requires the `.prefab.ron` extension;
- detects cycles with the visited source path and reports the full chain;
- limits nesting to **8 levels**;
- limits the whole dependency graph to **4096 nodes** across unique sources;
- loads a shared dependency once when the same source is referenced from more
  than one branch.

Each source file is also validated independently: version 1, 1..1024 nodes,
parent-before-child ordering, finite transforms, unique UUIDs, and bounded
components. A missing source, malformed RON, or failed traversal aborts the
command before the scene is changed.

Instantiation does not recursively expand nested Assemblies. A nested link is
copied as a link on the cloned node; the dependency walk exists to reject
invalid or cyclic source graphs before they enter the scene.

## Scene slots and the 10,000-node limit

The scene is a flat array with stable ids. Removing a node soft-clears its slot
instead of shifting later ids, so the slot remains allocated.

`assembly.update` and `assembly.revert` reuse the existing root subtree slots in
parent-before-child order:

- a same-size refresh rewrites nodes in place and consumes no new slots;
- a refresh that needs more nodes reuses all existing subtree slots first and
  appends only the missing nodes;
- a refresh that needs fewer nodes soft-removes the extra slots.

The commands calculate the number of slots a refresh would append and reject it
before mutation if the scene would exceed **10,000 slots**. A source that
shrinks does not free slots for later reuse, so repeatedly growing and shrinking
an instance can consume slots over time. Create a fresh project or remove
unrelated dead nodes if the hard limit is reached.

Reused child slots keep their UUIDs and authored stable keys. Newly created
children receive fresh UUIDs and do not inherit reconciliation keys from the
source. Internal entity references in copied source nodes are remapped to the
instance copies; external UUID references remain external.

## Non-goals for version 1

- No live inheritance, automatic refresh, or gameplay behavior defaults.
- No partial child override merge: `assembly.update` replaces children.
- No automatic expansion of nested Assembly sources.
- No source-file writing from `assembly.revert`, `assembly.update`, or
  `assembly.make_unique`.
- No background filesystem watcher behavior tied to the hash.
- No automatic repair of preserved root `Entity` variables whose target child
  disappeared during an update or revert.
