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
| `call CPad::UpdatePads` site | `0x53BEE6` -> `0x541DD0` | static, live | first call in CGame::Process; input injected after it |
| `IsAlreadyRunning` | `0x7468E0` | static, live | CreateEventA + ERROR_ALREADY_EXISTS; patched `xor eax,eax; ret` -> 2 clients ran |
| main loop background skip | `0x748A8D` (`je`, 6 bytes) | static, live | `cmp [ForegroundApp 0x8D621C],0; je` -> Sleep(100); NOPed |
| pause menu on focus loss | `0x53BC60` | static, live | only caller WndProc `0x748063`; patched to `ret` |
| LOGO -> next state imm | `0x748B0E` | static, live | `mov [gGameState], 2` at `0x748B08`; 5 skips movies |
| gGameState switch | `0x748AA1`, table `0x748EF8` | static | states 0-9 |
| menu flags | `0xBA677B` activate next frame, `0xBA67A4` active, `0xBA6831` start-game loading | static, live | WinMain states 6-8; clearing the first two in state 7 starts a game |
| `CTimer::m_UserPause` | `0xB7CB49` | live | stays 1 when the menu is left by flag writes; must be cleared |
| `CTheScripts::Process` | `0x46A000` | static, live | patched to `ret` on the first in-game frame in mp mode |
| `CRunningScript::Init` | `0x4648E0` (thunk -> `0x15626B0`) | static, live | zeroes fields; name at +8 |
| `CRunningScript::ProcessOneCommand` | `0x469EB0` | static, live | IP at +0x14, opcode u16, bit 15 = NOT |
| `CRunningScript` layout | base IP +0x10, IP +0x14, stack +0x18, sp +0x38, locals +0x3C, cond +0xC5, NOT +0xD2, isMission +0xDC, size 0xE0 | static, live | Init + CollectParameters |
| `CPad::GetPad` / pads | `0x53FB70`, `0xB73458 + n*0x134`, NewState at +0 | static, live | injected sprint moved CJ ~8 m/s |
| `RwD3D9Device` | `0xC97C28` | live | `IDirect3DDevice9*`; vtable may be a plugin's heap copy (WindowedMode swaps Reset); Present = slot 17 |
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

- **mp mode cannot disable main.scm before loading.** Its loading-time tick creates the player
  ped and the loading code dereferences it (crash at `0x571B73` reading `[0+0x36]`). Disable
  `CTheScripts::Process` on the first in-game frame instead, then dress CJ (`087B` x3 + `070D`):
  his body is otherwise only built by the intro mission and he renders invisible.
- **modloader is not multi-instance safe.** A second `gta_sa.exe` crashes in modloader's startup
  (reads/writes pointers from another process through a read-only file mapping). Not caused by the
  bridge; happens with `-nomods` and with modloader logging off.

- **Do not detour `Idle` (0x53E920) at its prologue.** Another plugin in a common mod setup (likely
  SilentPatch's frame-limiter patch on the `call` at `0x53E923`) rewrites bytes inside the first
  5 bytes after load, which corrupts a MinHook `jmp` and crashes on the first in-game frame.
  modloader also hooks both RsEventHandler call sites. Hooking the call sites and chaining to the
  previous target composes with both.
