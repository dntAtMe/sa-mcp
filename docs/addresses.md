# Address verification log

Target: `gta_sa.exe` 1.0 US, 14,383,616 bytes, PE timestamp `0x427101CA`, entry `0x824570`.

Status legend: **static** = confirmed by disassembling the exe; **live** = confirmed against a running
game (values read/written through the bridge and checked on screen); **ref** = from plugin-sdk /
gtamods, not yet confirmed here.

| Name | Address / offset | Status | Evidence |
|---|---|---|---|
| version check | `0x82457C == 0x94BF` | static | bytes `bf 94 00 00` |
| `call Idle` site | `0x53ECBD` -> `0x53E920` | static, live | RsEventHandler; hook pumps in-game frames |
| `call FrontendIdle` site | `0x53ECCB` -> `0x53E770` | static, live | RsEventHandler; hook pumps menu frames |
| `CWorld::Remove` | `0x563280` | static, live | calls `entity->Remove()` (vtbl+0xC); used by teleport |
| `CWorld::Add` | `0x563220` | static, live | reads `m_pRwObject` +0x18, matrix +0x14, placement +0x4 |
| `CWorld::Players` | `0xB7CD98`, stride `0x190` | static, live | `FindPlayerPed` @ `0x56E210` |
| clock hours / minutes | `0xB70153` / `0xB70152` | static, live | `CClock::Initialise` @ `0x52CD90`; set 21:15 -> night |
| weather forced/old/new | `0xC81318` / `0xC81320` / `0xC8131C` | static, live | `CWeather::ForceWeatherNow` @ `0x72A4F0`; set 8 -> rain |
| `CTimer::m_snTimeInMilliseconds` | `0xB7CB84` | static | read in `CClock::Initialise` |
| `CTimer::m_FrameCounter` | `0xB7CB4C` | live | tracks the bridge's own frame count |
| `gGameState` | `0xC8D4C0` | live | 7 in main menu, 9 in game |
| money / display money | `0xB7CE50` / `0xB7CE54` | live | HUD follows writes |
| ped / vehicle pools | `0xB74490` / `0xB74494`, sizes `0x7C4` / `0xA18` | live | sane model ids (0 = CJ, 400-611 vehicles), positions |
| `CPlaceable` placement / matrix | `+0x4` / `+0x14` | static, live | `CWorld::Add` |
| `CEntity` model | `+0x22` | live | |
| `CPed` flags bit 8 = bInVehicle, vehicle ptr | `+0x46C`, `+0x58C` | live | true on the intro BMX (model 481) |
| `CPed` health / armor | `+0x540` / `+0x548` | live | HUD bars follow writes |
| `CVehicle` driver / health | `+0x460` / `+0x4C0` | live | `is_driver` true on BMX, health 1000 |
| `CPhysical` move / turn speed | `+0x44` / `+0x50` | live (indirect) | zeroed on teleport, no residual motion |
| `CPed` max health | `+0x544` | ref | reads 100.14 at game start |
| `CEntity` area code | `+0x2F` | ref | |
| `CGame::currArea` | `0xB72914` | ref | |
| wanted ptr, level | `0xB7CD9C`, `+0x2C` | ref | |

## Compatibility notes

- **Do not detour `Idle` (0x53E920) at its prologue.** Another plugin in a common mod setup (likely
  SilentPatch's frame-limiter patch on the `call` at `0x53E923`) rewrites bytes inside the first
  5 bytes after load, which corrupts a MinHook `jmp` and crashes on the first in-game frame.
  modloader also hooks both RsEventHandler call sites. Hooking the call sites and chaining to the
  previous target composes with both.
