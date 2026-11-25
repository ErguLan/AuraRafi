# Audio System v1

Audio in AuraRafi is entity-scoped and deliberately bounded. It is not a full
DAW or a global streaming mixer.

## Runtime contract

- `AudioSource` lives on a `SceneNode`: `enabled`, `clip`, `autoplay`,
  `looping`, `volume`, `bus`, `spatial`, `min_distance`, `max_distance` and
  `rolloff`.
- A source linked to a block uses the node's world transform. When `spatial` is
  enabled, gain is computed from the active camera listener:
  full volume inside `min_distance`, silence beyond `max_distance`, and a
  bounded rolloff in between.
- The project mix exposes `master`, `music`, `sfx`, `voice` and `ambient` buses.
- The default voice budget is 8 and is configurable from 1 to 32.
- Playback is Windows PCM 16-bit WAV, mono/stereo, 8 kHz to 192 kHz, up to
  8 MiB per clip. MP3/OGG/FLAC files are catalogued but are not decoded by the
  current native output path.
- The editor can audition a WAV from Assets or from the Audio Inspector. The
  preview uses the same bounded Windows voice path and never starts Play.

## Authoring

Project Settings > Runtime exposes the mixer volumes and the 3D audio switch.
The Inspector exposes the source bus, spatial toggle, distances and preview
button. The Hierarchy shows an audio badge when a node has an enabled source.

```text
/game.audio action=attach target=Shelf clip=audio/checkout.wav bus=sfx spatial=true
/game.audio action=update target=Shelf volume=0.6 looping=true
/game.audio action=remove target=Shelf
```

## Scripting

```rhai
fn on_start() {
    let shelf = entity("Shelf");
    set_property(shelf, "audio.enabled", true);
    set_property(shelf, "audio.spatial", true);
    set_property(shelf, "audio.bus", "sfx");
    play_audio("Shelf");
    set_audio_bus_volume("sfx", 0.8);
}

fn on_destroy() {
    stop_audio("Shelf");
}
```

Available calls:

- `play_audio(name)`, `stop_audio(name)`, `stop_all_audio()`
- `set_volume(name, value)`
- `set_audio_bus_volume(bus, value)`
- `set_property(node, "audio.*", value)` for clip, enabled, looping, autoplay,
  spatial, bus, volume, min_distance and max_distance.

## Agent

The `audio_edit` tool maps to `game.audio` and accepts `attach`, `update` and
`remove`. `assets.catalog` lists audio files. The Agent can bind a clip to a
block without a second audio system.

## Not implemented

- Native decoding for MP3/OGG/FLAC.
- Waveform rendering in the Assets card.
- A dedicated mixer console with live meters.
- Occlusion, reverb zones, ducking and streaming.
