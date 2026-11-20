# WebRTC Control Plane

Status: v0.11 control plane. This document describes what is implemented in
`raf_net::webrtc` and, with equal precision, what is not.

The module implements SDP offer/answer handling, trickled ICE candidate lines,
a self-made signaling server and a bounded negotiation state machine. It does
not carry media. There is no DTLS, SRTP, SCTP, RTP, codec or congestion-control
implementation, and **browser interoperability is not implemented or claimed**.

## Plane boundary

| Layer | Status |
| --- | --- |
| SDP offer/answer model, builder, parser | Implemented |
| SDP type detection and validation | Implemented |
| ICE candidate model and attribute-line codec | Implemented (no ICE agent) |
| End-of-candidates marker | Implemented |
| Signaling transport (TCP + line-delimited JSON) | Implemented |
| Token/session handshake | Implemented, 5 s window |
| Negotiation state machine and timeouts | Implemented |
| STUN binding requests and connectivity checks | Not implemented |
| TURN allocation | Not implemented |
| DTLS certificate, handshake, fingerprint verification | Not implemented |
| SRTP keying and packet protection | Not implemented |
| SCTP association and data channels | Not implemented |
| RTP packetization, jitter buffer, RTCP | Not implemented |
| RAF1 video encoding/decoding | Not implemented |
| VP8/H264/AV1 encoding/decoding | Not implemented |
| Audio capture, encoding and transport | Not implemented |
| Congestion control and pacing | Not implemented |
| Browser interop | Not implemented, not claimed |

Everything in the implemented column is the control plane. It produces and
consumes text and JSON; it never produces a UDP datagram.

## SDP contract

`build_offer` emits one deterministic BUNDLE session. The exact offer shape is:

```text
v=0
o=- <session-id> 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0 1
a=ice-ufrag:<ufrag>
a=ice-pwd:<pwd>
a=fingerprint:sha-256 <AA:BB:...>
a=setup:actpass
m=application 9 UDP/DTLS/SCTP webrtc-datachannel
c=IN IP4 0.0.0.0
a=mid:0
a=sctp-port:5000
a=max-message-size:262144
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:1
a=sendonly
a=rtpmap:96 RAF1/90000
a=rtcp-mux
```

`build_answer` mirrors the offer's BUNDLE mids, media order, payload types and
rtpmaps, uses the answerer's ICE credentials and fingerprint, and changes:

- `a=setup:actpass` becomes `a=setup:active`;
- the video direction becomes `a=recvonly`.

The answer builder ignores `SdpOfferParams::video_payload_type`; the offer's
payload type is the authority for a mirrored answer. An offer whose video
section is `recvonly` or `inactive` cannot be answered with a `recvonly`
section and is rejected.

### Offer or answer tag

SDP has no explicit offer/answer field. This control plane derives the tag from
`a=setup`:

- `actpass` means offer;
- `active` or `passive` means answer.

`SessionDescription::validate` requires the label on the struct to agree with
the text. An answer may never use `actpass`.

### RAF1

`RAF1` is an AuraRafi-internal payload label at a 90 kHz clock. It is a
placeholder for the future internal video pipeline. No encoder or decoder for
it exists in this repository, and no browser or external SDP peer knows how to
decode it. The generated offer must not be presented as a browser-playable
stream.

The parser accepts other RTP encoding names so a future media layer can carry
the descriptor, but it does not implement any codec. The parser understands the
SDP structure, mids, payload types and rtpmaps; it does not turn any of them
into media.

### Parser requirements

`parse_session_description` rejects a document that is:

- larger than 64 KiB, or has a line larger than 1 KiB;
- not ASCII, or not starting with `v=0`;
- missing `o=`, `s=`, `t=`, `a=ice-ufrag`, `a=ice-pwd`,
  `a=fingerprint:sha-256`, `a=setup` or `a=group:BUNDLE`;
- missing `a=mid` on a media section;
- using a media kind other than `application` or `video`;
- using an unsupported media protocol, a port other than 9, or more than one
  section of the same kind;
- missing `a=sctp-port` on the data-channel section;
- missing `a=rtcp-mux` on the video section;
- listing a video payload type without a matching `a=rtpmap`;
- listing a BUNDLE mid without a media section, or a media mid outside the
  BUNDLE group.

Attributes that are not part of this contract (`extmap`, `ssrc`, `fmtp`,
`msid`, `ice-options`, and future attributes) are ignored. SDP-level
`a=candidate` lines are ignored; candidates travel through the signaling
channel.

## ICE contract

`IceCandidate` stores:

```text
foundation, component, transport, priority, address, port, typ, related_address
```

`parse_candidate` accepts `candidate:...` or `a=candidate:...` and enforces:

- foundation 1..=32 characters from the ICE character set;
- component 1..=2;
- only the `UDP` transport (case-insensitive on input, uppercase on output);
- non-zero priority;
- address 1..=64 characters and a valid IPv4 or IPv6 literal;
- non-zero candidate port;
- `raddr` and `rport` only together, and never on a host candidate;
- `srflx` and `relay` candidates require a related endpoint;
- no unknown extension tokens.

`related_address` stores the complete related endpoint in `address:port` form
because the related port is mandatory with `raddr`. IPv6 endpoints are stored
with brackets, for example `[2001:db8::2]:40000`.

`to_candidate_line` renders the canonical `a=candidate:...` line without a
line terminator. `parse_end_of_candidates` recognizes both
`a=end-of-candidates` and `end-of-candidates`; it is provided for a
line-oriented candidate import path, while the JSON signaling message set
carries individual candidates only.

This is a codec, not an ICE agent. Nothing sends or receives a STUN binding
request, computes a candidate pair, performs a connectivity check, or nominates
a pair.

## Signaling server

`SignalingServer` is a non-blocking TCP listener. Each connection is one
line-delimited JSON stream. `SignalPeer::poll` reads available bytes, decodes
complete lines, validates every message and returns decoded
`SignalingMessage` values. `SignalPeer::send` serializes one message, queues it
in a bounded queue and attempts a non-blocking flush. Accepted sockets use
`nodelay`.

### Message shapes

The enum uses the default externally tagged JSON representation:

```json
{"Hello":{"protocol":1,"token":"<uuid>","session":"<uuid>"}}
{"Offer":{"sdp_type":"offer","sdp":"v=0\r\n..."}}
{"Answer":{"sdp_type":"answer","sdp":"v=0\r\n..."}}
{"Candidate":{"foundation":"1","component":1,"transport":"UDP","priority":2130706431,"address":"127.0.0.1","port":50000,"typ":"host","related_address":null}}
{"Bye":{"reason":0}}
{"Error":{"code":2,"message":"signaling token mismatch"}}
```

`Bye.reason` uses the shared `raf_net::protocol::bye_reason` values, 0 through
8. `Error.code` uses the `signal_code` constants.

### Handshake

1. The server accepts a connection and starts a 5-second handshake timer.
2. The client sends `Hello` with protocol 1 and the expected token and session
   instance.
3. The server compares the token in constant time, without logging or echoing
   it, and compares the session instance.
4. A mismatch or an unsupported protocol queues an `Error` message and closes
   the connection.
5. Only after a valid `Hello` may either side send offer, answer or candidate
   messages. The server never sends `Hello`.

The decoded `Hello` handed back by `SignalPeer::poll` still contains the peer
token. Do not log, echo or persist that value.

- `Hello` on an already-open connection, a non-Hello first message, malformed
  JSON, a failed message validation and an oversized line all close the
  connection with a bounded error. A local send queue that is full returns
  `WouldBlock`; it never grows. A close is graceful only while the queue can
  drain.

`signal_code` values:

| Code | Name | Meaning |
| --- | --- | --- |
| 1 | `PROTOCOL_MISMATCH` | Wrong signaling protocol |
| 2 | `TOKEN_MISMATCH` | Wrong token |
| 3 | `SESSION_MISMATCH` | Wrong session instance |
| 4 | `HANDSHAKE_TIMEOUT` | No valid `Hello` inside 5 s |
| 5 | `MALFORMED_MESSAGE` | Invalid JSON, failed validation, or bad order |
| 6 | `LINE_TOO_LARGE` | Line exceeded the 64 KiB limit |

### Limits

| Bound | Value |
| --- | --- |
| Signaling line | 64 KiB |
| Signaling send queue | 64 messages |
| Signaling send queue bytes | 4 MiB |
| Messages decoded per poll | 64 |
| Read budget per poll | 64 KiB |
| Handshake window | 5 s |
| Error message | 256 bytes |
| SDP document | 64 KiB |
| SDP line | 1 KiB |
| Media sections | 8 |
| Video payload types | 8 |
| Session id | 64 bytes |
| Mid | 32 bytes |
| ICE ufrag/pwd | 4..=256 bytes |
| Fingerprint | 95 characters (32 SHA-256 bytes) |
| Candidate line | 512 bytes |
| Candidate address | 64 bytes |
| Candidate foundation | 32 bytes from the ICE character set |
| Candidate component | 1..=2 |
| ICE ufrag/pwd characters | ASCII letters, digits, `+` or `/` |
| Remote candidates per session | 32 |
| Negotiation window | 30 s |
| Connection window | 15 s |

The signaling line limit applies to the JSON encoding, so a very large SDP
document can pass the 64 KiB SDP check and still be rejected when JSON escaping
pushes its line over 64 KiB. Normal SDP documents are far below either bound.

The signaling transport preserves line order. It does not decide whether an
`Offer` or `Answer` is semantically in order; `WebRtcSession` does that.

The signaling server does not terminate TLS. Bind it to loopback, or place it
behind an operator-terminated tunnel with authentication. It is not a public
listener by itself.

## Negotiation state machine

```text
Offerer:
  Idle --on_local_offer--> WaitingAnswer --on_remote_answer--> Gathering
       --on_remote_candidate--> Connecting --on_connected--> Connected

Answerer:
  WaitingOffer (new_answerer) --on_remote_offer--> Gathering
       --on_remote_candidate--> Connecting --on_connected--> Connected
```

- `on_remote_candidate` is accepted in `WaitingAnswer`, `Gathering` and
  `Connecting`. Duplicates are ignored. Before an offer it is an ordering
  error. After `Connected` or `Closed` it is ignored.
- At most 32 remote candidates are retained. The 33rd distinct candidate is a
  limit error.
- `on_remote_answer` requires a stored local offer and validates the answer
  against it: BUNDLE mids, media kinds, media order, mids and video payload
  types must mirror the offer.
- `on_connected` is a report from a future media layer. This module never calls
  it by itself and never invents a connected path.
- `tick(now)` reports at most one event. Pending events are reported first. If
  the active deadline has elapsed, the session closes, queues a `Closed` event
  and returns `NegotiationTimeout` or `ConnectTimeout`. `tick` does not sleep;
  the caller owns the loop clock.

Events:

| Event | Meaning |
| --- | --- |
| `NegotiationTimeout` | The offer/answer exchange did not complete inside 30 s |
| `ConnectTimeout` | No connected path inside 15 s after the offer/answer exchange |
| `Connected` | `on_connected` was accepted |
| `Closed` | The session is terminal |

The event queue is structurally bounded: only `Connected`, `Closed` and one
timeout event can be queued by a valid transition.

## What remains for a real media plane

The control plane is intentionally transport-agnostic. A future media layer can
consume the same public types without changing them. The gates, in order, are:

1. **ICE agent.** Send STUN binding requests over the signaled UDP candidate
   pairs, implement role checks, nomination and consent freshness, and call
   `WebRtcSession::on_connected` only after a nominated pair succeeds. Gate: a
   host/srflx pair is selected over loopback with packet captures.
2. **DTLS.** Generate an ephemeral certificate, perform the DTLS handshake on
   the selected path, verify the peer certificate against the SDP fingerprint,
   and expose the verified fingerprint. Gate: both sides complete the handshake
   and a tampered fingerprint is rejected.
3. **SRTP.** Export SRTP keying material from DTLS, protect RTP and RTCP, and
   enforce replay windows. Gate: protected RTP/RTCP round-trip and a replayed
   packet is rejected.
4. **SCTP and data channels.** Establish the SCTP association over DTLS,
   implement DCEP open/ack, enforce the advertised `a=max-message-size`, and
   provide ordered and unordered delivery. Gate: a data-channel echo.
5. **RTP and a codec.** Replace the `RAF1` placeholder with an implemented
   encode/decode path, packetization, keyframes, fragmentation and a jitter
   buffer, plus RTCP feedback. Gate: a decoded frame matches the source frame
   after loss and reordering.
6. **Congestion control.** Pace packets, measure loss and delay, adapt bitrate,
   and degrade or drop video before input. Gate: bounded latency under forced
   loss.
7. **Browser interop.** Only after gate 5 with a codec a browser supports. The
   current `RAF1` offer will not be decoded by a browser. Gate: an external
   browser connects and renders a frame; this gate does not exist today.

Until gate 7 passes, no documentation, diagnostic or log may describe the
current path as browser-compatible.

## Local verification

Run the crate's WebRTC tests:

```bash
cargo test -p raf_net --lib webrtc
```

The tests cover:

- offer round-trip, stable line order and parsed fields;
- answer mirroring and answer/offer mismatch rejection;
- missing `a=ice-ufrag` and oversized SDP rejection;
- invalid offer parameters;
- host, server-reflexive and IPv6 candidate round-trips;
- malformed candidate and end-of-candidates handling;
- a loopback signaling exchange of `Hello`, `Offer`, `Answer`, `Candidate` and
  `Bye`;
- server `close()` sending a normal `Bye`;
- wrong token and protocol mismatch rejection with an `Error` message;
- an oversized signaling line being rejected;
- the happy-path negotiation, negotiation timeout, connect timeout,
  answer-before-offer rejection, candidate-list bound and candidate-after-close
  behavior.

A manual control-plane probe only needs a TCP socket and the JSON shapes above:
send `Hello`, then `Offer`, read `Answer` and `Candidate`, and close with `Bye`.
No media will flow. A browser cannot complete this path as a media peer because
the media plane is absent and `RAF1` is unknown to it.
