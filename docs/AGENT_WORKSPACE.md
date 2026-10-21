# Native Agent workspace

The Hub's **Agents / Agentes** section opens project-scoped conversations without
mounting a Game viewport or entering Play. **Work with AI / Trabajar con IA** is
also available in a project's Hub menu and in the project creator. The editor's
application bar opens the same workspace; its Agent dock shares the same state.

Normal **Open / Abrir** in the Hub opens the editor directly. Its worker loading
indicator and retry/cancel error state stay in the Hub; they do not navigate to
Agents or select a saved conversation's document. **Open editor / Abrir editor**
inside Agents targets that conversation's document, checks the save receipt,
releases the old UI input owner and explicitly requests an editor frame. A save
failure is shown as an error, not left spinning indefinitely. The pending handoff
reserves its document so a background run cannot continually start another save
and starve opening the editor.

## Read-only 3D preview

**3D view / Vista 3D**, identified by a framed-cube icon, is an optional tab
beside Changes, Artifacts and Checks. It renders the selected conversation's
Game document, not whichever world happened to be last mounted in the editor.
When that document is mounted, it borrows the editor's live scene; otherwise
it borrows the service-owned scene. There is no second scene owner, document
clone, editor workbench, gizmo, construction tool, script execution or Play.
Electronics shows an explicit unavailable state rather than a fake 3D board.

The preview creates its SceneRenderer/RenderRuntime only when first rendered.
It reuses the window's ApiGraphicBasic graphics context and native compositor.
Camera and scene changes invalidate the retained output; unchanged UI frames
reuse it, including CPU pixel uploads. Perspective camera state is transient
and never updates project cameras or saves. Right drag orbits; right-drag with
WASD/QE translates; the wheel zooms; Frame resets the view to scene bounds.
Only an explicit pointer gesture in the canvas can acquire navigation input.
Text fields, modals and focus loss cannot leave movement active.

Project, conversation and right-panel dividers share clamped geometry, an
8-point hit target and a horizontal-resize cursor. Drag adjusts live layout;
Escape cancels a drag, arrows adjust a focused divider and Home resets it.
Compact layouts collapse the conversation list before crushing the chat.
Widths are transient application state, not project document settings.

Hiding the preview stops all rendering immediately and retains resources for
five seconds to support quick tab switches. The event loop schedules one
expiration wakeup, not a hidden rendering loop. Leaving Agents, changing its
document/project, closing its project or recreating the native graphics host
releases resources immediately, including the compositor's retained source.
Re-entering Agents does not automatically reopen the preview.

Preview targets are capped at 1024x768 (GPU) or 480x360 (CPU), use Efficient
geometry/textures, disable shadow maps, and apply an ApiGraphicBasic surface
budget (64 MiB GPU resource admission, 512 cached meshes, 64 textures).
This is not a total-process RAM guarantee or native performance acceptance.
The preview does not yet advertise an Agent viewport-capture tool.

## Project residency

Starting at the Hub creates an empty `AgentService`; no project documents,
histories, asset catalogs or model requests are loaded by that service yet.
Opening a project (for Agents or the editor) loads its metadata, session
registry, conversation history and all of its authoring documents, then scans
its asset catalog on a worker. This is more than interface-only loading but
does not create a viewport or submit a model request. A project passed on the
command line is opened explicitly at startup.

The project rail shows a green dot for a loaded host, gray for an unloaded
project and amber while loading/closing. The dot is residency, not proof of
an active model run. Each project can be closed independently. **Start all**
and **Close all** require separate confirmation dialogs. Bulk opening is
serialized to one project-loading worker and preserves the selected project;
it never submits prompts or starts Play. Bulk closing cancels queued loads and
Agent submissions/runs, waits for document saves and the history writer, then
releases authoring hosts/leases. A failed save retains the host with retry.
Closing an editor-mounted project first returns its document to the service
and navigates back to the Hub. No project files or conversations are deleted.
Cancellation is cooperative: an already-sent provider request cannot be
recalled from the remote server, but its result can no longer execute tools.

The scheduler skips inactive conversations; project observations are built
only for new submissions, not for HTTP/streaming wait ticks. The attached
executor borrows the node graph rather than cloning it on each poll. Agent
polling and visible repainting remain separate: an unchanged wait tick does
not request another editor frame. The command catalog is reused by the service.
These are bounded-work changes, not measured native FPS/GPU acceptance.

`AgentService` supervises project work outside redraw callbacks. Each loaded
project owns a shared `AgentPanel`, asset jobs, a project write lease, a ledger,
and `ProjectDocumentHost`s identified by the canonical session registry UUID.
Project loading and document serialization run on worker threads. Conversation
IDs, document IDs, and run/tool IDs remain distinct.

Selecting another conversation parks its runtime; it does not cancel its request,
discard its draft, or change its world/design target. Returning to the Hub hands
the live document back to the service. Opening the editor mounts that same
in-memory document after the save barrier. There is no second native scene owner
and no internal CLI/MCP process. Navigation does not authorize a mutation.

## Saving and review

Active-mode writes require operation review by default. A review contains the
tool, arguments/available native preview, destination and expected revision.
Approving one operation does not approve subsequent operations. A stale reviewed
revision produces a conflict, not a silent write. Existing Inspect/Plan behavior
and activity presentation are retained. The review toggle is explicit.

Applied document changes enter a worker save queue. The Agent receives its final
tool result only after a durable save receipt. The scene, node graph and, for
Electronics, schematic and PCB are journaled together. Each file is published
with a recoverable previous generation; interrupted publications roll forward
on the next open. External file changes pause saving rather than overwriting
them. A failed save remains retryable and blocks the next document operation.
Chats sharing a document serialize writes and receipt consumption.

History stores messages, unsent drafts, document binding, task snapshots/events and the last
acknowledged revision in `.ai/agent_history.ron`. After a process restart an
unfinished task is shown as interrupted; **Continue / Continuar** makes a fresh
inspection request. It never blindly replays an old mutation queue.

Project opening and new conversation context preparation show an animated
loading ring. Reduced-motion preferences disable its animation. New interface
copy uses the existing flat English/Spanish locale files.

## Boundaries

- The application still uses native RafUI and ApiGraphicBasic, with the Hub's
  visual direction; no web frontend or additional UI framework was introduced.
- A closed viewport cannot provide visual capture. The native headless context
  does not advertise `viewport_capture`; structural verification stays available.
- Evidence tabs show real tool diffs, artifact references and verification
  payloads. Absence of verification is not displayed as success.
- This is an in-process native document host, not a precompiled preview/export
  format, a separate engine version launcher, or a new headless CLI mutation API.
- Documents are loaded into memory with a limit of 64 sessions per project;
  project eviction and lazy document loading are not implemented here.
- Background conversations are supported; specialist subagents, ACP adapters,
  workflow scheduling and a project-skills manager remain future work.
- The authoring lease is cooperative. Older engine/CLI versions do not acquire
  it; the loader checks live attached endpoints and saves detect external file
  changes. Avoid parallel external writers to these same document files.

Native UI commands (`agent.workspace`, `agent.workspace.editor`, review tabs and
save retry) are presentation intents, not additions to the public command catalog.
