# Native Agent image tools

Image generation extends the existing image queue and Assets catalog. It does
not add an asset database, a second renderer, separate Chat/Image/Vision model
settings, or an obligatory linear workflow.

## Routes

- `asset_generate_local_png`: the existing offline icon/badge/sprite/texture
  generator; drawing behavior is unchanged.
- `asset_generate_image`: the active conversation provider, endpoint, API key
  and model. `model` is an optional explicitly selected image-only generator,
  not an automatic fallback or a second Settings slot. OpenRouter modalities
  and image endpoint capabilities are discovered from provider catalogs. An
  unsupported model fails before paid generation. OpenAI-compatible providers
  without modality metadata remain unknown until their image API responds.
- `external_studio_install` / `external_studio_generate`: available only when
  the optional external studio is enabled in AI Providers. The Agent chooses
  when to delegate, and may inspect or generate again after a result.

The native Agent, Console and attached CLI/MCP use the same asset service and
image enqueue boundary. External commands are `/asset.studio_install` and
`/asset.studio_generate`; file jobs expose status/cancel commands rather than
claiming scene transaction support.
Its runtime polls an in-flight call without executing it twice or returning a
queued placeholder as a completed tool result. Existing live activity shows
the current operation, run elapsed time and tool count. Stop, changing chats,
switching projects or closing the editor cancels owned child processes.

Ready images have unique names in `assets/generated`, bounded PNG dimensions,
metadata and a verified decode before the existing Assets catalog refreshes.
Generation does not change scene entities, assign textures, increment the scene
revision or issue an Undo token. Failed or cancelled requests can still incur
provider charges; stopping a child does not revoke a request already received
by a remote provider. No paid inference is necessary for offline tests.

## Optional external studio

AI Providers contains one optional external-studio section:

- public HTTPS repository, without embedded credentials;
- absolute separate installation directory (its parent must already exist);
- runtime executable, such as `node`;
- CLI entry point relative to that installation.

All fields are user configuration. Raf ships no studio source, vendor URL or
application-specific entry point. Enabling the section explicitly trusts
external executable code with the current user's OS permissions and temporarily
passed provider credentials. This is **not a sandbox**. Do not enable an
unreviewed repository. Downloads use shallow Git clone, no overwrite, no shell,
no automatic dependency installation and no package hooks. Existing local
checkouts are reused only if their configured entry point resolves inside them.
Incomplete downloads are retained for manual recovery, never recursively deleted.

The CLI receives `--request <absolute JSON> --result <absolute JSON>`.
The request contains `prompt`, `model`, `generation_mode`, `local_style`, `size`,
`transparent`, `output_path` and `metadata_path`. Credentials are environment
variables `RAF_IMAGE_PROVIDER`, `RAF_IMAGE_BASE_URL`, `RAF_IMAGE_API_KEY`, never
request JSON. It must write the requested PNG/metadata and a result object
`{"ok":true}` or `{"ok":false,"error":"..."}`. Output is PNG-only in this
slice: JPEG/WebP/SVG are rejected, not silently renamed or converted.

An independent studio may expose the same implementation through its MCP
server. Raf currently invokes the standalone CLI; it does not spawn Raf CLI or
route its own native tools through MCP. Publishing a studio's compatible CLI
to its configured Git repository is separate from local implementation.
