# Address verification log

Target: `gta_sa.exe` 1.0 US, 14,383,616 bytes, PE timestamp `0x427101CA`, entry `0x824570`.

Status legend: **static** = confirmed by disassembling the exe; **live** = confirmed against a running
game; **ref** = from plugin-sdk / gtamods, not yet confirmed here.

| Name | Address / offset | Status | Evidence |
|---|---|---|---|
| version check | `0x82457C == 0x94BF` | static | bytes `bf 94 00 00` |
| `Idle` | `0x53E920` | static | called from RsEventHandler `0x53ECBD`; prologue `sub esp,8` |
| `FrontendIdle` | `0x53E770` | static | called from RsEventHandler `0x53ECCB` |
| `CWorld::Remove` | `0x563280` | static | calls `entity->Remove()` (vtbl+0xC), checks entity type at +0x36 |
| `CWorld::Add` | `0x563220` | static | reads `m_pRwObject` +0x18, matrix +0x14, placement +0x4 |
| `CWorld::Players` | `0xB7CD98`, stride `0x190` | static | `FindPlayerPed` @ `0x56E210` |
| clock hours / minutes | `0xB70153` / `0xB70152` | static | `CClock::Initialise` @ `0x52CD90` |
| weather forced/old/new | `0xC81318` / `0xC81320` / `0xC8131C` | static | `CWeather::ForceWeatherNow` @ `0x72A4F0` |
| `CTimer::m_snTimeInMilliseconds` | `0xB7CB84` | static | read in `CClock::Initialise` |
| `CPlaceable` placement / matrix | `+0x4` / `+0x14` | static | `CWorld::Add` |
| money / display money | `0xB7CE50` / `0xB7CE54` | ref | |
| wanted ptr, level | `0xB7CD9C`, `+0x2C` | ref | |
| `CTimer::m_FrameCounter` | `0xB7CB4C` | ref | |
| `gGameState` | `0xC8D4C0` | ref | |
| `CGame::currArea` | `0xB72914` | ref | |
| ped / vehicle pools | `0xB74490` / `0xB74494`, sizes `0x7C4` / `0xA18` | ref | |
| `CEntity` model / area | `+0x22` / `+0x2F` | ref | |
| `CPhysical` move / turn speed | `+0x44` / `+0x50` | ref | |
| `CPed` flags (bit 8 bInVehicle), health, max health, armor, vehicle | `+0x46C`, `+0x540`, `+0x544`, `+0x548`, `+0x58C` | ref | |
| `CVehicle` driver / health | `+0x460` / `+0x4C0` | ref | |
