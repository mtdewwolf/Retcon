# Local protocol v1

`v1.json` is the source of truth for Retcon's local RPC message shapes. Frames
are UTF-8 JSON objects terminated by one newline; transports reject frames over
1 MiB before decoding.

Connections send `auth`, then exchange `client.hello` and `server.hello`. Both
peers must support protocol version 1. Features are opt-in: a sender may use a
feature only when both hello messages advertise it. `request.cancel` is
best-effort and has no response; `ping` must receive `pong`.

On reconnect, clients authenticate and negotiate again, then use
`events.replay` from their last retained sequence. Future v1 changes must be
additive. Changing or removing a field requires a new schema version. Tokens
and secret request parameters must never be logged; binary artifacts travel
out-of-band through opaque references.
