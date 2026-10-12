<a name="readme-top"></a>

<br />
<div align="center">
  <img src="assets/cairn-icon.png" alt="Cairn" width="140">
  <h3 align="center">Cairn Client</h3>
  <p align="center">One launcher to play single-player games together.</p>
</div>

## Table of Contents

- [About The Project](#about-the-project)
  - [Built With](#built-with)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Installation](#installation)
- [Credits](#credits)

## About The Project

**Cairn Client** turns single-player games into shared ones. One player hosts, a friend joins through Steam, and both of you
play in the same world. Each game is described by a small *game profile* (its executable, Steam app id, mod payload folder, menu
picture and server), so the launcher, the session handling and the setup checks are shared and a title is added as a profile
rather than as a new launcher.

Cairn Client is an **unofficial** project and a modified version of an earlier mod; it is not affiliated with any game's studio
or publisher. See AUTHORS.md for the complete list of contributors and historical upstream lineage.

### Built With

* [![.NET](https://img.shields.io/badge/.NET-8-512BD4?logo=dotnet&logoColor=white)](https://dotnet.microsoft.com/)
* [![C++](https://img.shields.io/badge/C++-native_plugin-00599C?logo=cplusplus&logoColor=white)](https://isocpp.org/)
* [![Lua](https://img.shields.io/badge/Lua-game_scripts-2C2D72?logo=lua&logoColor=white)](https://www.lua.org/)
* [![Blazor](https://img.shields.io/badge/Blazor-launcher-512BD4?logo=blazor&logoColor=white)](https://dotnet.microsoft.com/apps/aspnet/web-apps/blazor)
* [![Inno Setup](https://img.shields.io/badge/Inno_Setup-6-1F6FEB)](https://jrsoftware.org/isinfo.php)

## Getting Started

### Prerequisites

* Windows 10 or 11
* The **.NET 8 Desktop Runtime** and the **Microsoft Edge WebView2 Runtime** (the launcher's window is Blazor hosted in WebView2)
* The **Visual C++ runtimes** the game and the native plugin need (the launcher's setup checklist tells you if one is missing)
* Steam, with the game you want to play installed (see [Supported titles](#supported-titles))

### Installation

Download the latest installer (`.exe`) from the [Releases](https://github.com/Cairn-client/launcher/releases) page and run it.

The installer is not code-signed yet, so Windows may warn: click **More info**, then **Run anyway**.

The installer carries only the launcher. Each game's mod is a separate download: open the game in the launcher and click **Download mod**
(a small file from the same release, checked against a checksum before it is used). Games bought outside Steam can be pointed at
with **Locate game...** on their page. See [docs/PACKS.md](docs/PACKS.md).

## Credits

See [AUTHORS.md](AUTHORS.md) for the complete list of contributors and historical upstream lineage. Cairn Client is free software under the GNU General Public License version 3, see [LICENSE](LICENSE).

<p align="right">(<a href="#readme-top">back to top</a>)</p>
