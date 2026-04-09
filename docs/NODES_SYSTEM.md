# AuraRafi Visual Scripting Architecture (Nodes System)

The **Visual Scripting System** within AuraRafi allows users to build "no-code" logic through connected nodes and wires, reminiscent of industry-standard blueprint environments. It's designed to be completely independent, purely mathematical, and lightweight.

## 1. State and Core Data (`raf_nodes`)
The data model lives primarily inside `raf_nodes`.
- **`NodeGraph`**: A structure encapsulating an array of `Node` elements and an array of `Connection` elements (wires joining pins).
- **`Node`**: The fundamental logic block. It contains its `NodeId` (a UUID), 2D `position`, a categoric label, and an array of `NodePin`s.
- **`NodePin`**: Terminals where wires connect. Defined as either an `Input` or an `Output` kind, possessing a specific `PinDataType` (like Flow, Bool, Int, Float, String).

### Multi-Flow Capabilities
In earlier iterations, the engine only held a singular global graph. Now, the state holds a `graphs: Vec<NodeGraph>` list. This allows the user to have multiple contextual event trees (e.g. "On Player Death", "Weather Loop", etc.) decoupled from each other.

## 2. The GUI Interaction (`raf_editor`)
The `NodeEditorPanel` (housed in `crates/raf_editor`) controls all interactivity.
Since `egui` processes UI element hits sequentially, the editor implements critical physics fixes:
- **Input Swallowing Protection**: The underlying canvas layer uses `ui.allocate_space()` and intercepts clicks *before* the nodes are drawn dynamically on top. If `egui` checks collision sequentially, nodes painted later automatically block background hits, bypassing the "input swallowing" bug completely.
- **Ray-cast Pin Connections**: Drag connections use `any_released()` and literal `Rect::contains()` to mathematically test if the mouse pointer released a wire *exactly* atop an opposing pin's bounding box instead of relying on `hovered()`. This stops drag focus locks.
- **Z-Index Selection**: Clicking on a node's header flags it as the `selected_node`. Rendering loops evaluate `selected_node` against the iterative list and explicitly apply rendering highlights, avoiding heavy internal state machines. 

## 3. History and Undo/Redo Engine
Deep-cloning history allows real-time iteration.
Any user mutation (moving a node, bridging a wire, creating a flow, deleting a node) sets `state_changed = true`. At the end of the frame display cycle, if changed, the entire array of `graphs` deep-clones into `history: Vec<(Vec<NodeGraph>, usize)>`.
Users traverse this timeline traversing the `history_pointer` index back and forth upon hitting `Ctrl+Z` and `Ctrl+Y`.

## 4. Internationalization (i18n)
All strings rendered in the Node Editor side-panels and palette are injected using the engine's translation framework (`raf_core::i18n::t()`), rendering UI in `en.json` or `es.json` depending on active context, upholding the "Zero-If" policy.
