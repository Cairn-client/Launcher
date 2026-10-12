# The bridge: how a game's mod talks to the launcher

The launcher is the same for every game. What is particular to one game (how to read its state, how to change it, what its menu holds)
lives in a program that ships **in the game's pack**: the pack's `companion` (docs/PACKS.md, external mods). That program, the
**bridge**, talks to the launcher over a local named pipe with a small, versioned protocol. When a game needs something new, its bridge
and its pack change and are released; the launcher is released only when the protocol itself gains something.

`Cairn.Setup/BridgeProtocol.cs` is the protocol, `Cairn.Setup/BridgeServer.cs` the launcher's end of the pipe.

## Connecting

The launcher starts the companion with the environment variable `CAIRN_BRIDGE_PIPE` set to a pipe name (`cairn-bridge-<32 hex>`, random
for each launcher run). The companion opens `\.\pipe\<name>` and speaks one JSON object per line, UTF-8, at most 16 KB a line. The pipe
is open to the current Windows user only. The first thing a bridge says is `hello`; anything else first ends the connection.

| Bridge to launcher | |
| --- | --- |
| `{"t":"hello","protocol":1,"game":"noita"}` | first, always. A `protocol` newer than the launcher's is answered with `bye` and the connection ends. |
| `{"t":"menu","title":"..","blocked":"","note":"..","items":[ ... ]}` | the whole menu. An item: `id` (letters, digits, `-`, `_`), `group`, `kind` (`onoff` `choice` `key` `action` `info`), `on` (true/false/absent), `locked`, `disabled`, `value`, `label`, `help`, `gate` (`f1`..`f12`). At most 64 items; a damaged item is skipped. |
| `{"t":"notice","text":"Ann joined^^Press Insert"}` / `{"t":"notice_hide"}` | the cards at the top left; `^^` separates lines. |
| `{"t":"subtitle","seconds":4,"text":".."}` | a spoken line (1 to 15 seconds). |

| Launcher to bridge | |
| --- | --- |
| `{"t":"welcome","protocol":1}` | the answer to a `hello` this launcher speaks. |
| `{"t":"attach"}` | the launcher draws the menu now: the mod's own key handling steps aside. Sent until the first `menu`. |
| `{"t":"state"}` | send the menu again (asked about once a second while the menu is open). |
| `{"t":"do","id":"friendly_fire","verb":"enter"}` | the player did something to a row (`enter` or `nudge`). |
| `{"t":"bye","reason":".."}` | the connection is ending. |

Texts are cut to their limits and stripped of control characters; unknown messages are ignored, so a newer bridge can say more than an
older launcher understands. The look of the menu (Nordath style, the Cairn colours) is the launcher's: a bridge sends content, never
styling.

## What this does not carry yet

Saves (the slot list, new/continue/delete) and the setup steps a game needs are still Kingdom Come II code in the launcher; they are
the next parts to become data and messages.
