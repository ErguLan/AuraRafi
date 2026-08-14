# Game authoring commands

Inspect first with `engine.status`, `project.info`, `capabilities.list`,
`game.describe_scene`, `script.list`, and bounded `workspace.*` commands.

Useful attached scene commands:

- `game.add`: create a primitive with a safe name and optional transform;
- `game.select`: select by stable id or name;
- `game.rename`: rename a selected/targeted entity;
- `game.duplicate`: duplicate a node with an explicit offset;
- `game.set_transform`, `game.move`, `game.rotate`, `game.scale`: update a
  transform deterministically;
- `game.delete`: remove an explicitly selected/targeted entity;
- `session.list`, `session.create`, `session.open`, `session.duplicate`:
  isolate worlds inside one project;
- `project.save`: queue a checkpoint for project and session documents.

Useful scripting commands:

- `script.create lang=rhai name=controller`;
- `script.attach file=scripts/controller.rhai entity=Horse`;
- `script.detach file=scripts/controller.rhai entity=Horse`;
- `script.list`;
- `script.validate file=scripts/controller.rhai`;
- `script.compile_nodes flow=Main output=scripts/generated.rhai`.

`script.run` is an editor-only authoring check for `on_start`; it does not
start the product Runtime. Check the catalog returned by `capabilities.list`
before assuming a future terrain, material, animation, or particle command
exists.
