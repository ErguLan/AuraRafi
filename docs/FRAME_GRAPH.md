# Frame Graph

`crates/raf_agb/src/frame_graph.rs` is the backend-neutral
scheduling contract for multi-pass rendering. The standalone probe compiles a
`FramePlan` for scene, overlays and output. The active `BasicDevice` scene path
has yet to adopt this contract. See the [product migration guide](AGB_MIGRATION.md).

## Why

The current scene renderer records one command list and presents one target.
Shadows, post-processing, PBR, particles and skeletal deformation all need
multiple passes, transient targets and explicit synchronization. Adding those
passes ad hoc would duplicate resource ownership and make backend parity
unmeasurable.

The frame graph owns:

- resource lifetimes (imported vs transient);
- pass ordering derived from declared reads and writes;
- barriers between neutral resource states;
- transient memory aliasing with a hard byte budget;
- validation errors instead of silent corruption.

It does not own: backend objects, command encoding, shader compilation or
presentation. A backend consumes `CompiledFrameGraph` and maps neutral states
to its own API.

## Contract

```text
FrameGraph::new(budget)
  import_texture(desc)     // swapchain, persistent scene target, canvas
  create_texture(desc)     // transient, aliased, budgeted
  add_pass(PassDesc)       // name, kind, reads, writes
  compile_into(&mut out)   // order + barriers + alias slots
```

Rules enforced by the compiler:

- every read of a transient has a writer earlier in the graph;
- a pass cannot write the same resource twice;
- a graph cannot exceed 4096 resources, 1024 passes or 128 accesses per pass;
- depth textures only accept depth-attachment or shader-read writes;
- transient memory above the budget is a compile error;
- a failed compile clears the scratch graph so no partial frame reaches a
  backend.

## Aliasing

Transient resources are grouped by liveness interval `[first_pass, last_pass]`.
Two resources share a slot only when their intervals do not overlap and their
format, sample count, dimensions, mip count and usage are compatible. The slot
keeps the largest descriptor so every resource that reuses it fits.

This is what keeps shadow maps, post targets and future PBR targets from
allocating a full-resolution texture each.

## Neutral formats and states

Formats: `Rgba8Unorm`, `Rgba8UnormSrgb`, `Bgra8Unorm`, `Bgra8UnormSrgb`,
`Rgba16Float`, `R16Float`, `Depth32Float`, `Depth24PlusStencil8`.

States: `Undefined`, `ColorAttachment`, `DepthAttachment`, `ShaderRead`,
`ShaderWrite`, `CopySrc`, `CopyDst`, `Present`.

No graphics API type appears in the public contract. Vulkan, DX12, Metal and
WebGPU execute it through AGB.

## Integration order (when authorized)

1. Map `CompiledFrameGraph` to backend barriers and attachments through the
   native ApiGraphicBasic product contract.
2. Move shadow-map preparation to a graph pass.
3. Move the optimized filter and future post chain to graph passes.
4. Reuse transient resources by `AliasSlot` under the graphics device budget.
5. Expose pass timings and transient bytes in `SceneFrameMetrics`.

## Acceptance

- the same scene produces the same pixels through the graph and the current
  single-pass path;
- barrier count does not grow when a pass is unused;
- transient bytes stay under the configured budget on potato;
- idle frames allocate nothing;
- CPU recovery keeps equivalent document semantics.
