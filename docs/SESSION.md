# A game's session, run by its pack

Hosting and joining a game is the game's business: which relay to start, how the game is started, what is loaded into it, what talks to the other player.
For Kingdom Come: Deliverance II all of that is the mod's own programs (relay, agent, native plugin), and since launcher 0.3.18 it is **driven by the mod's
program too**, so a change to how a session works is a mod release, not a launcher release.

A pack opts in with `"session": true` in `pack.json` (it needs an `optionsProgram`, see [PACKS.md](PACKS.md), and is a `kind: "mod"` pack). The launcher then starts
`<program> session` the first time the player presses Host or Join, speaks to it in JSON lines, and ends it when the game is over, before the mod is updated
or removed, and when the launcher closes. A pack without the flag keeps the launcher's own code, which is used whenever the flag is not there.

The program is separate from the in-game menu **bridge** ([BRIDGE.md](BRIDGE.md)): that one starts with the game and draws the mod's menu; this one starts before the
game and owns the connection. The game, the relay and the agent it starts are programs of their own, left running when the session program ends.

## Transport

Standard input and output of the program, one JSON object per line (UTF-8, at most 64 KB a line). The program writes nothing else on standard output.
Anything either side does not understand is ignored. Nothing is started before the first `config`.

## Launcher to program

| `t` | Fields | Meaning |
| --- | --- | --- |
| `config` | `gameExe`, `hostPort`, `infoPort`, `versionPort`, `steamAppId`, `allowSteam`, `relayReuse`, `connectGate`, `injectDelaySeconds`, `menuTakeover`, `agentPath`, `relayPath`, `dllPath`, `startSaves` | the player's settings; sent before a command that uses them. Out-of-range values are replaced by defaults. |
| `relay` | | make sure this install's relay is up (reuse it when it is the same release and settings, replace an older copy of ours, never touch another program's) |
| `relay_stop` | | stop the relay |
| `launch` | `ip`, `port`, `steam` (code or empty), `ping` (ms or -1), `skipModCheck` | start the game connected to a host: address, or Steam code |
| `connect` | | the player pressed CONNECT (or the menu did): load the plugin into the game and start the agent |
| `choice` | `choice`: `bring`, `fresh` or `fresh:<style>` | the joiner's first-join answer |
| `fallback` | `host`, `port` | after a failed Steam connection: the agent again, with the host's address |
| `cancel` | | forget the launch in progress (the game is not closed) |
| `friends` | | ask which Steam friends are hosting |
| `test` | `ip`, `port`, `steam` | test the connection to a host |
| `close_game` | | close the game and its helper programs |
| `stop` | | end |

## Program to launcher

| `t` | Fields | Meaning |
| --- | --- | --- |
| `hello` | `protocol` (1) | first line |
| `state` | `stage`, `status`, `connect{enabled,reason}`, `conn{text,bad}`, `join{state,message,canFresh}`, `host{relay,error,steamState,steamMessage,steamCode,refused}`, `game` | **everything the launch panel shows, whole**, sent each time something changes. `stage` is `idle`, `waiting-connect`, `injecting`, `verifying`, `connected` or `failed`. |
| `game` | `pid`, `startedAt` (ISO, local), `hosting` | the game was started: the launcher starts the in-game menu and the start-game flow for it |
| `game_exit` | | the game is over; the relay, the agent and the Modding Tools' helpers have been stopped |
| `problem` | `title`, `text`, `next`, `reportBug`, `launchAnyway` | a start Windows refused or a file that is gone: shown with Report a bug (and LAUNCH ANYWAY for the mod's own files) |
| `message` | `title`, `text`, `next`, `addressFallback` | a plain message; `addressFallback` offers the host's address after a failed Steam connection |
| `error` | `text` | a plain error |
| `toast` | `text` | a short confirmation |
| `refused` | `text`, `key` | a player tried to join with another version (once per `key`) |
| `version_mismatch` | `text` | a peer runs another release |
| `friends` | `state`, `message`, `next`, `friends[{name,code,release}]` | the answer to `friends` |
| `log` | `line` | a line for the launcher's log (redacted there) |

The launcher copies `state` into the fields its screens already read, so the launch panel, the host window and the banners are the same with or without a session program.

## The start-game flow (launcher 0.3.19)

The session also owns **Start Game / Join Game on the game's main menu**: before the game starts it checks the first-run pages are accepted, stages the host's start save and lists the
player's worlds (through the agent's helper); while the game sits at its main menu it draws the menu through the game's console, reads the player's choices from the game's log and carries
them out (a load, a new adventure, the joiner's first-join answer), connects the host by itself once the loaded world has settled, and puts the mod's logo in for that game only. The
launcher's part is `config.menuTakeover` (the player's setting), `config.startSaves` (where the start saves are) and showing that the flow is on. A pack that asks for launcher 0.3.19 in its
`launcherMin` does this; older launchers do it themselves and are not given such a pack.
