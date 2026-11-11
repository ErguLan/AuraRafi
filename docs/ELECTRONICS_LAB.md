# Electronics laboratory

Open the lab from Electronics using **Open lab** or **Connect board**. The
Simulation tab runs a DC analysis of the current schematic. Select a component
and a pin to inspect its voltage or choose Branch current. The circuit overlay
highlights the selected pin's net once the current revision has been solved.

In Devices, search for ports and choose the baud rate before connecting. A USB
descriptor can identify a vendor or adapter. The board's firmware confirms its
identity and capabilities by replying to the protocol handshake. A passive
board requires external instrumentation that can report its measurements.

Select a measured channel and a schematic probe to link them. Comparison
requires matching units and a simulation of the current revision. For voltage,
the hardware measurement must use the same reference node as the simulation.
The lab keeps the most recent 4,096 samples per connection. A capture that
outlasts that buffer marks the discarded prefix. The Captures tab plays and
seeks through the recorded data. Session storage retains the latest 16 captures
and DC runs.

The Agent can call `electronics_lab_simulate`, `electronics_lab_probe`, and
`electronics_lab_signals` in Electronics projects. Its serial tool reads
status and measurements. Device output changes are initiated through Devices.

## Serial protocol, version 1

Messages are UTF-8 JSON, one object per newline, up to 4,096 bytes per line.
The default port setting is 115200 baud, 8N1. The editor timestamps each sample
on receipt. Channel names and units stay stable within a connection.

| Direction | Example |
| --- | --- |
| Editor → board | `{"type":"identify","protocol":1}` |
| Board → editor | `{"type":"hello","protocol":1,"board":"Arduino Uno","firmware":"lab-1","outputs":[{"id":"led","min":0,"max":1}]}` |
| Board → editor | `{"type":"sample","channel":"analog","value":311,"unit":"count"}` |
| Editor → board | `{"type":"set","output":"led","value":1,"id":1}` |
| Board → editor | `{"type":"ack","id":1,"ok":true}` |

Only advertise outputs that the firmware actually handles. Send an ACK after
the output was applied, or `ok:false` for a rejected value. A missing ACK
leaves the output state unknown. On disconnect, the editor stops its connection
worker. Safe output behavior on a disconnected physical board belongs in its
firmware.

Firmware with OTA support can include
`"ota":{"target":"esp32","max_image_bytes":1310720}` in its hello. The editor
checks a chosen .bin against the advertised size limit, computes its SHA-256,
and exports a preparation manifest. The firmware's updater still validates the
image and target before installation.

The [reference sketch](../examples/electronics/raf_serial_lab/raf_serial_lab.ino)
reports an ADC reading on Arduino Uno and classic ESP32 development boards.
Configure `RAF_ADC_PIN` and `RAF_LED_PIN` for the actual board. Use an input
within the ADC's permitted range and a shared ground. The reading is an
uncalibrated ADC count. The sketch advertises the onboard LED only when its pin
is defined.
