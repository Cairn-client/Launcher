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
| `{"t":"menu","title":"..","blocked":"","note":"..","back":false,"items":[ ... ]}` | the whole menu (`"back": true` marks a submenu: Back is sent to the bridge, not closing the menu). An item: `id` (letters, digits, `-`, `_`), `group`, `kind` (`onoff` `choice` `key` `action` `info`), `on` (true/false/absent), `locked`, `disabled`, `value`, `label`, `help`, `gate` (`f1`..`f12`). At most 64 items; a damaged item is skipped. |
| `{"t":"notice","text":"Ann joined^^Press Insert"}` / `{"t":"notice_hide"}` | the cards at the top left; `^^` separates lines. |
| `{"t":"subtitle","seconds":4,"text":".."}` | a spoken line (1 to 15 seconds). |
| `{"t":"saves","slots":[{"id":"run-12","name":"Ann's run","detail":"depth 4","kind":"world","when":"2026-10-12T10:30:00Z"}]}` | the saves the game has, the whole list each time (`kind`: `world` or `character`; ids are letters, digits, `- _ .`; at most 100). Shown in the Saves row of the game's page while the game runs. |

| Launcher to bridge | |
| --- | --- |
| `{"t":"welcome","protocol":1}` | the answer to a `hello` this launcher speaks. |
| `{"t":"attach"}` | the launcher draws the menu now: the mod's own key handling steps aside. Sent until the first `menu`. |
| `{"t":"back"}` | the player pressed Back in a menu the bridge sent with `"back": true` (a submenu of its own): the bridge decides where that goes; the launcher does not close the menu. |
| `{"t":"state"}` | send the menu again (asked about once a second while the menu is open). |
| `{"t":"do","id":"friendly_fire","verb":"enter"}` | the player did something to a row (`enter` or `nudge`; a nudge to the left also says `"dir":-1`). |
| `{"t":"saves_ask"}` | send the list of saves again (sent when the bridge connects). |
| `{"t":"save_delete","id":"run-12"}` | the player confirmed deleting this save; the bridge deletes it in its game and answers with the new `saves` list. |
| `{"t":"bye","reason":".."}` | the connection is ending. |

Texts are cut to their limits and stripped of control characters; unknown messages are ignored, so a newer bridge can say more than an
older launcher understands. The look of the menu (Nordath style, the Cairn colours) is the launcher's: a bridge sends content, never
styling.

## What this does not carry yet

The setup steps a game needs (find its tools, link files) are still Kingdom Come II code in the launcher, and Kingdom Come II itself
still talks to the launcher through its log and console, not through a bridge. Both are the next parts to move.
