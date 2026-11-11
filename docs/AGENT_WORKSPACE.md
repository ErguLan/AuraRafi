# Native Agent workspace

## Harness implementation pass (2026-10-23)

Native command receipts are verified before the model-facing result is compacted.
The internal command request retains operation/entity limits but has no wire-result
byte cap; `AgentToolResult::model_content` still bounds provider-facing output.
Parent postconditions prefer the live graph rather than a receipt's parent field.
Mixed scripting/build prompts reserve scripting tools within the bounded
36-tool contextual budget. Unknown tools report the names actually available for
the turn.

Workspace and dock share compact expandable tool rows. Multi-tool turns collapse
intermediate activity into a details group without hiding failures or the final
answer. Expanding activity never changes persisted domain messages. Virtual heights
use the same bounded Markdown tiles as presentation: headings, lists and fenced
monospace code; code indentation and punctuation survive. This is a Markdown subset,
not HTML, link execution or a full syntax-highlighted editor.

The composer accepts Ctrl+V image paste (normal text paste wins when available)
and PNG/JPEG/WebP drops **onto the composer**, in both native surfaces. A plus
button explicitly pastes an image when clipboard text would otherwise win. Image
input is automatic: the engine classifies known model families, blocks families
known to be text-only, and attempts unknown models so the provider remains the
authority. No manual confirmation is required. Changing model/provider clears the
manual reasoning override. No network request or ingestion starts on launch.

Images are decoded/normalized off the UI thread to at most 1024×1024 PNG, 4 MiB each,
four per message. File imports accept up to 16 MiB sources with decoder dimension
limits of 4096×4096; clipboard input is bounded to 16 megapixels. Immutable UUID
copies live in `.ai/agent_media`; history and per-conversation drafts store only
relative references and dimensions, never pixel/base64 data. Storage is bounded to
256 files/64 MiB per project. Removal detaches a reference, not the stored file;
automatic orphan cleanup is not implemented. Import completions target the session
that initiated them, not whichever conversation is selected later. The composer
shows metadata/removal chips, not image thumbnails.

Provider workers resolve project-confined references and send actual OpenAI-format
`image_url` data-URI content parts. Only images belonging to the latest user turn
are sent (and retained during its tool loop); text-only follow-ups do not resend
old images. The configured provider receives these images. Stored references do
not prove that the provider/model understood them. Successful viewport and scene
captures are queued as ephemeral evidence and attached to the next model request
when the model family accepts images. They are not persisted as user messages;
text-only follow-ups do not resend them. A provider image rejection surfaces an
explicit error instead of pretending the image was inspected.

Inactive manual limit fields are hidden rather than showing misleading greyed-out
numbers. Output is automatic by default: the native runtime omits the response token cap
unless the custom-limit switch is on. This is **not infinite output**; provider,
model, context and cost limits remain. Automatic reasoning omits overrides and
sampling parameters. Low/medium/high are explicit overrides, available in settings
and via the composer's cycle button; OpenRouter and OpenAI-compatible endpoints use
their respective field dialects. Unsupported overrides surface errors without paid
silent retries. Settings edit one provider at a time and distinguish local setup
from connection testing. Native Claude/GenAI protocol adapters are not added.

The history estimate counts text only, not provider token usage or image tokens.
The 128-step safety guard ends in Error, not Done, and instructs inspection before
continuing. Provider length/content-filter stops and interrupted streams also fail
explicitly; incomplete tool calls are not executed as successful completions.

Automatic context compaction/checkpoints, capability detection, provider connection
tests, world-generation recipes and model-driven visual verification remain pending.
Native-window acceptance, actual provider vision compatibility and performance
measurements are separate from focused unit/CPU-surface checks.

### Focused verification (2026-10-23)

- 52 editor tests passed: harness regressions, conversation navigation, Active
  permissions, compact tool/activity projection, long-message scroll/atlas,
  image ingestion, model settings and conditional manual-limit controls.
- 44 `raf_ai` unit tests and 8 CLI/MCP tests passed. These are not an attached
  engine/MCP end-to-end run or a paid provider compatibility test.
- A 760x640 Spanish dark CPU-surface fixture was rendered and visually inspected.
  It is a UI fixture, not evidence of a model run or native clipboard input.
- Locale JSON and scoped diff whitespace checks passed. Editor validation used
  a command-local `profile.dev.package.raf_editor.opt-level=0` override; persisted
  build profiles were not changed. This is not a performance benchmark.

The application build attempt failed because the C: temporary volume was full.
Focused editor tests then completed using process-local TEMP/TMP on D:. No user
files were deleted or global Windows settings changed. No updated application
executable is delivered by this pass; the user requested tests only afterward.

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
The workspace Agent also exposes `viewport_capture`. Because that surface has no
live viewport, captures use the bounded CPU offscreen path and still return a
project-local PNG plus ephemeral model image evidence. Current-frame capture
(`view=current`) remains editor-only.

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

Active mode authorizes native tool execution without per-operation approval.
The legacy review preference cannot override Active; its review controls and
approval cards are hidden in both the workspace and editor dock. Selecting
Active also releases already pending reviews, including parked conversations.
Inspect remains read-only and Plan remains a sandboxed preview. Project scope,
command validation, revision/idempotency, undo and durable saving remain enforced;
Active does not enable unconfigured external tools or Play/Runtime.
The low-level runtime still supports explicit scoped review for other callers;
approved review snapshots retain their stale-revision conflict checks.

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

## Electronics laboratory

Open the lab from Electronics with **Open lab** or **Connect board**.
**Back to editor** returns to the mounted document. The lab shares the
Agent workspace and palette. Simulation, Devices and Captures are its
main views, with Chat available in its own tab.

- **Simulation** runs the DC solver against the current schematic. Select
  a component and pin for node voltage or select Branch current. The map
  highlights the selected net and the readings come from the solved revision.
  The LED model is piecewise linear. Transient analysis is outside this scope.
- **Devices** enumerates serial ports. Choose the port and baud rate and
  connect. USB descriptors identify an adapter or vendor, and the firmware
  handshake reports the board and its capabilities. A passive PCB requires
  an instrument that reports measurements. Link a channel to a schematic
  probe with matching units. Comparing voltages also requires the same
  reference node and a simulation of the current circuit revision.
- **Captures** records and replays measurements. The live connection keeps
  up to 4,096 samples and marks any discarded prefix in a longer recording.
  Session storage retains the latest 16 captures and 16 DC runs.
- The Agent can call `electronics_lab_simulate`, `electronics_lab_probe` and
  `electronics_lab_signals` in Electronics. The serial tool reads status and
  measurements. Output changes are initiated through Devices.

### Serial protocol v1

Messages are UTF-8 JSON, one object per newline, up to 4,096 bytes per line.
The default is 115200 baud, 8N1. The editor timestamps samples on receipt.
Channel names and units stay stable within a connection.

| Direction | Example |
| --- | --- |
| Editor → board | `{"type":"identify","protocol":1}` |
| Board → editor | `{"type":"hello","protocol":1,"board":"Arduino Uno","firmware":"lab-1","outputs":[{"id":"led","min":0,"max":1}]}` |
| Board → editor | `{"type":"sample","channel":"analog","value":311,"unit":"count"}` |
| Editor → board | `{"type":"set","output":"led","value":1,"id":1}` |
| Board → editor | `{"type":"ack","id":1,"ok":true}` |

Advertise only outputs the firmware handles. Send an ACK after applying the
output or `ok:false` if rejected. A missing ACK leaves the state unknown.
Disconnect stops the worker. The firmware owns safe output behavior after
disconnect.

Firmware can advertise OTA preparation with
`"ota":{"target":"esp32","max_image_bytes":1310720}` in its hello. The editor
checks the selected binary against the advertised size, computes SHA-256 and
exports a manifest. The firmware updater validates the image and target and
handles installation.

The [reference sketch](../examples/electronics/raf_serial_lab/raf_serial_lab.ino)
reports uncalibrated ADC counts on Arduino Uno and classic ESP32 development
boards. Set `RAF_ADC_PIN` and optionally `RAF_LED_PIN` for the actual board.
Use a shared ground and keep the input within the ADC range.

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
