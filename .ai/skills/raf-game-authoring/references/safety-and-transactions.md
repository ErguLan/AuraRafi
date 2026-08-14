# Safety and transaction rules

Use this sequence for every mutation:

```text
inspect -> dry-run -> confirm -> apply -> inspect result -> validate -> save
```

- `expected_revision` rejects writes over a newer manual edit.
- `idempotency_key` makes retries safe and prevents duplicate entities/files.
- `confirm=true` is required for writes; `dry_run=true` never changes the
  document.
- `transaction_id`, `revision`, `diff`, `verification`, and `metrics` are
  evidence fields. Report them to the caller.
- Attached scene mutations return an `undo_token` only when the editor has
  recorded the real scene snapshot. Call `transaction.undo` with that token and
  `confirm=true` to restore it.
- An undo token is scoped to the issuing project, active session, and exact
  revision. Any later edit invalidates it. Never store it as a project secret.
- Keep paths project-relative and reject traversal or arbitrary shell commands.
- Bound generated entities, script writes, workspace bytes, and tool calls for
  potato hardware.
- Do not enable Play, Stop, Runtime, or pretend that an unsupported graphics
  system was generated.
