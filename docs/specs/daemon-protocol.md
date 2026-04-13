# Lspee Daemon Wire Protocol (Control + Stream)

## Status

Draft v1.

## Goals

- Define a versioned JSON protocol for daemon control messages.
- Define stream channel envelope used for LSP payload transport.
- Standardize error codes and retry behavior.
- Keep protocol compatible with the lifecycle spec in `daemon-session-lifecycle.md`.

## Versioning

- Protocol version is `1` for this spec.
- Every control message MUST include `v`.
- Unknown `v` MUST return `E_UNSUPPORTED_VERSION` and close the connection.
- Backward-incompatible changes require incrementing `v`.

## Encoding + Framing

### Control channel framing

- Transport: local socket (`unix` / named pipe), per lifecycle spec.
- Payload: UTF-8 JSON object.
- Framing: newline-delimited JSON (NDJSON), one message per line.
- Maximum frame size: 1 MiB (`1048576` bytes).
- Oversized frame SHOULD return `E_FRAME_TOO_LARGE` then close.

### Stream channel framing

- Stream endpoint returned by `AttachOk`.
- Uses same NDJSON framing and max frame size.
- Payload uses `StreamFrame` envelope below.

## Common Control Envelope

All control messages use this shape:

```json
{
	"v": 1,
	"id": "c_01JX...",
	"type": "Attach",
	"payload": {}
}
```

Fields:

- `v` (integer, required): protocol version.
- `id` (string, required): client-generated request id (unique per connection).
- `type` (string, required): message discriminator.
- `payload` (object, required): type-specific body.

### Response mapping

- For request `id = X`, daemon MUST respond with exactly one terminal message:
  - success type (`AttachOk`, `ReleaseOk`, etc.) with same `id`, or
  - `Error` with same `id`.
- Notifications (`Ping`, `Pong`, events) MAY omit `id`.

## Control Message Types

Implementation note: canonical serde wire structs and protocol constants for control messages live in `crates/lspee_protocol/src/lib.rs`.

## 1) Attach (request)

`type = "Attach"`

```json
{
	"v": 1,
	"id": "req_1",
	"type": "Attach",
	"payload": {
		"session_key": {
			"project_root": "/repo/app",
			"config_hash": "sha256:3f2e...",
			"lsp_id": "rust-analyzer"
		},
		"client_meta": {
			"client_name": "lspee",
			"client_version": "0.4.0",
			"pid": 81234,
			"cwd": "/repo/app"
		},
		"capabilities": {
			"stream_mode": ["dedicated", "mux_control"]
		}
	}
}
```

Validation:

- `session_key.project_root`: non-empty absolute path string.
- `session_key.config_hash`: non-empty string (`algo:digest` recommended).
- `session_key.lsp_id`: lowercase `[a-z0-9-_\.]+`.
- `client_meta.pid`: positive integer if present.

## 2) AttachOk (response)

`type = "AttachOk"`

```json
{
	"v": 1,
	"id": "req_1",
	"type": "AttachOk",
	"payload": {
		"lease_id": "lease_01JX...",
		"session_id": "sess_01JX...",
		"stream": {
			"mode": "dedicated",
			"endpoint": "unix:///run/user/1000/lspee/stream/lease_01JX.sock"
		},
		"server": {
			"state": "Ready",
			"reused": true
		}
	}
}
```

Rules:

- `lease_id` is required and unique daemon-wide while active.
- If `mode = "mux_control"`, `endpoint` MAY be omitted and stream uses control channel frames.

## 3) Release (request)

`type = "Release"`

```json
{
	"v": 1,
	"id": "req_2",
	"type": "Release",
	"payload": {
		"lease_id": "lease_01JX...",
		"reason": "client_exit"
	}
}
```

`reason` enum (optional):

- `client_exit`
- `client_disconnect`
- `shutdown`
- `error`

## 4) ReleaseOk (response)

`type = "ReleaseOk"`

```json
{
	"v": 1,
	"id": "req_2",
	"type": "ReleaseOk",
	"payload": {
		"lease_id": "lease_01JX...",
		"ref_count": 0
	}
}
```

## 5) Call (request/response)

Synchronous JSON-RPC forwarding over an existing lease.

Request (`type = "Call"`):

```json
{
	"v": 1,
	"id": "req_3",
	"type": "Call",
	"payload": {
		"lease_id": "lease_01JX...",
		"request": {
			"jsonrpc": "2.0",
			"id": 1,
			"method": "workspace/symbol",
			"params": { "query": "main" }
		}
	}
}
```

Response (`type = "CallOk"`):

```json
{
	"v": 1,
	"id": "req_3",
	"type": "CallOk",
	"payload": {
		"lease_id": "lease_01JX...",
		"response": {
			"jsonrpc": "2.0",
			"id": 1,
			"result": {}
		}
	}
}
```

## 6) Ping / Pong

`Ping` may be sent by either side:

```json
{ "v": 1, "id": "req_3", "type": "Ping", "payload": { "ts_ms": 1770012345678 } }
```

`Pong` response:

```json
{ "v": 1, "id": "req_3", "type": "Pong", "payload": { "ts_ms": 1770012345678 } }
```

## 7) Stats (request/response)

Request:

```json
{ "v": 1, "id": "req_4", "type": "Stats", "payload": {} }
```

Response (`StatsOk`):

```json
{
	"v": 1,
	"id": "req_4",
	"type": "StatsOk",
	"payload": {
		"sessions": 2,
		"leases": 3,
		"uptime_ms": 120034,
		"counters": {
			"sessions_spawned_total": 5,
			"sessions_reused_total": 12,
			"sessions_gc_idle_total": 2,
			"session_crashes_total": 1,
			"attach_requests_total": 17
		}
	}
}
```

## 8) Shutdown (request/response)

Request:

```json
{ "v": 1, "id": "req_5", "type": "Shutdown", "payload": {} }
```

Response (`ShutdownOk`):

```json
{ "v": 1, "id": "req_5", "type": "ShutdownOk", "payload": { "accepted": true } }
```

## 9) Error (terminal response)

`type = "Error"`

```json
{
	"v": 1,
	"id": "req_1",
	"type": "Error",
	"payload": {
		"code": "E_SESSION_RESTARTING",
		"message": "Session is terminating; retry attach.",
		"retryable": true,
		"details": { "retry_after_ms": 150 }
	}
}
```

Fields:

- `code` (string, required)
- `message` (string, required, human-readable)
- `retryable` (boolean, required)
- `details` (object, optional)

## Error Codes

Protocol-level:

- `E_UNSUPPORTED_VERSION` — unknown `v`.
- `E_BAD_MESSAGE` — invalid JSON, missing required fields, wrong types.
- `E_FRAME_TOO_LARGE` — frame exceeds max size.
- `E_UNKNOWN_TYPE` — unsupported `type`.
- `E_TIMEOUT` — request processing timeout.
- `E_INTERNAL` — unhandled daemon failure.

Attach/session-level:

- `E_INVALID_SESSION_KEY` — malformed session key fields.
- `E_SESSION_SPAWN_FAILED` — LSP process launch failed.
- `E_SESSION_INIT_FAILED` — initialize handshake failed.
- `E_SESSION_RESTARTING` — session in `Terminating`; client should retry.
- `E_DAEMON_SHUTTING_DOWN` — daemon not accepting new attaches.
- `E_RESOURCE_LIMIT` — capacity/FD/process limit reached.

Lease-level:

- `E_LEASE_NOT_FOUND` — unknown or already-released lease id.
- `E_LEASE_OWNERSHIP` — lease does not belong to caller connection/user.

Security/compatibility:

- `E_AUTH_FAILED` — local auth/permission failure.
- `E_PERMISSION_DENIED` — socket/ACL denied.

## Stream Channel Schema

`StreamFrame` envelope:

```json
{
	"v": 1,
	"type": "LspIn",
	"lease_id": "lease_01JX...",
	"seq": 41,
	"payload": {
		"jsonrpc": "2.0",
		"id": 9,
		"method": "textDocument/hover",
		"params": {
			"textDocument": { "uri": "file:///repo/app/src/lib.rs" },
			"position": { "line": 2, "character": 7 }
		}
	}
}
```

Frame fields:

- `v` (integer, required)
- `type` (string, required): one of:
  - `LspIn` (client -> daemon -> backend LSP)
  - `LspOut` (backend LSP -> daemon -> client)
  - `StreamError` (terminal stream error)
- `lease_id` (string, required)
- `seq` (integer, required): monotonically increasing per stream direction.
- `payload` (object, required for `LspIn`/`LspOut`)

`StreamError` example:

```json
{
	"v": 1,
	"type": "StreamError",
	"lease_id": "lease_01JX...",
	"seq": 52,
	"payload": {
		"code": "E_SESSION_CRASHED",
		"message": "Underlying LSP process exited",
		"retryable": true
	}
}
```

## JSON Schema (normative, draft 2020-12)

```json
{
	"$schema": "https://json-schema.org/draft/2020-12/schema",
	"$id": "https://lspee.dev/schemas/daemon-control-v1.json",
	"title": "Lspee Daemon Control Message v1",
	"type": "object",
	"required": ["v", "type", "payload"],
	"properties": {
		"v": { "const": 1 },
		"id": { "type": "string", "minLength": 1 },
		"type": {
			"type": "string",
			"enum": [
				"Attach",
				"AttachOk",
				"Release",
				"ReleaseOk",
				"Call",
				"CallOk",
				"Ping",
				"Pong",
				"Stats",
				"StatsOk",
				"Shutdown",
				"ShutdownOk",
				"Error"
			]
		},
		"payload": { "type": "object" }
	},
	"allOf": [
		{
			"if": { "properties": { "type": { "const": "Attach" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["session_key", "client_meta"],
						"properties": {
							"session_key": {
								"type": "object",
								"required": ["project_root", "config_hash", "lsp_id"],
								"properties": {
									"project_root": { "type": "string", "minLength": 1 },
									"config_hash": { "type": "string", "minLength": 1 },
									"lsp_id": { "type": "string", "pattern": "^[a-z0-9._-]+$" }
								},
								"additionalProperties": false
							},
							"client_meta": { "type": "object" },
							"capabilities": { "type": "object" }
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "AttachOk" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["lease_id", "stream"],
						"properties": {
							"lease_id": { "type": "string", "minLength": 1 },
							"session_id": { "type": "string", "minLength": 1 },
							"stream": {
								"type": "object",
								"required": ["mode"],
								"properties": {
									"mode": {
										"type": "string",
										"enum": ["dedicated", "mux_control"]
									},
									"endpoint": { "type": "string" }
								},
								"additionalProperties": false
							},
							"server": { "type": "object" }
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "Release" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["lease_id"],
						"properties": {
							"lease_id": { "type": "string", "minLength": 1 },
							"reason": {
								"type": "string",
								"enum": [
									"client_exit",
									"client_disconnect",
									"shutdown",
									"error"
								]
							}
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "Call" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["lease_id", "request"],
						"properties": {
							"lease_id": { "type": "string", "minLength": 1 },
							"request": { "type": "object" }
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "CallOk" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["lease_id", "response"],
						"properties": {
							"lease_id": { "type": "string", "minLength": 1 },
							"response": { "type": "object" }
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "Shutdown" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "ShutdownOk" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["accepted"],
						"properties": {
							"accepted": { "type": "boolean" }
						},
						"additionalProperties": false
					}
				}
			}
		},
		{
			"if": { "properties": { "type": { "const": "Error" } } },
			"then": {
				"required": ["id"],
				"properties": {
					"payload": {
						"type": "object",
						"required": ["code", "message", "retryable"],
						"properties": {
							"code": { "type": "string", "minLength": 1 },
							"message": { "type": "string", "minLength": 1 },
							"retryable": { "type": "boolean" },
							"details": { "type": "object" }
						},
						"additionalProperties": false
					}
				}
			}
		}
	],
	"additionalProperties": false
}
```

## Retry Guidance

- Client SHOULD retry `Attach` for `retryable=true` errors with jittered backoff.
- For `E_SESSION_RESTARTING`, initial retry delay SHOULD be 100-250ms.
- Client MUST NOT retry on protocol/validation errors unless request is corrected.
