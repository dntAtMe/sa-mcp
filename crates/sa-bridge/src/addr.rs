//! GTA San Andreas 1.0 US (gta_sa.exe, PE timestamp 0x427101CA) addresses and struct offsets.
//!
//! "[static]" = confirmed by disassembling the exe. "[live]" = confirmed in a running game.
//! Everything else comes from community references (plugin-sdk, gtamods wiki); treat as
//! unverified until marked. See docs/addresses.md.

// --- version detection ---
/// [static] `*(u32*)0x82457C == 0x94BF` on 1.0 US (plugin-sdk's check).
pub const VERSION_CHECK: u32 = 0x82457C;
pub const VERSION_CHECK_US10: u32 = 0x94BF;

// --- functions ---
/// [static] `call Idle` (Idle = 0x53E920, `void Idle(void*)`) in RsEventHandler: in-game frames.
pub const CALL_IDLE: u32 = 0x53ECBD;
/// [static] `call FrontendIdle` (0x53E770) in RsEventHandler: menu frames.
pub const CALL_FRONTEND_IDLE: u32 = 0x53ECCB;
/// [static] `call CPad::UpdatePads` (0x541DD0), first thing in CGame::Process (0x53BEE0).
/// Pad overrides are applied right after it.
pub const CALL_UPDATE_PADS: u32 = 0x53BEE6;
/// [static] `bool IsAlreadyRunning()` — CreateEventA + ERROR_ALREADY_EXISTS. Called from WinMain 0x74872D.
pub const IS_ALREADY_RUNNING: u32 = 0x7468E0;
/// [static] WinMain state LOGO: `mov [gGameState], 2` at 0x748B08; imm32 at +6. Writing 5 skips
/// the logo/title/intro movies straight to FRONTEND_LOADING.
pub const LOGO_NEXT_STATE_IMM: u32 = 0x748B0E;
/// [static] WinMain main loop: `cmp [ForegroundApp 0x8D621C], 0; je +0x320` (6 bytes at
/// 0x748A8D) skips the frame and Sleep(100)s while the window is in the background.
pub const MAINLOOP_BACKGROUND_JE: u32 = 0x748A8D;
/// [static] Opens the pause menu on focus loss (`if (!0xBA6831 && !menuActive)
/// activateMenuNextFrame = 1`). Only caller: WndProc deactivate path at 0x748063.
pub const PAUSE_ON_FOCUS_LOSS: u32 = 0x53BC60;
/// [static] `void CTheScripts::Process()` (cdecl). Patched to `ret` in mp mode.
pub const THE_SCRIPTS_PROCESS: u32 = 0x46A000;
/// [static] `void CRunningScript::Init()` (thiscall; Hoodlum thunk -> 0x15626B0).
pub const RUNNING_SCRIPT_INIT: u32 = 0x4648E0;
/// [static] `bool CRunningScript::ProcessOneCommand()` (thiscall).
pub const RUNNING_SCRIPT_PROCESS_ONE: u32 = 0x469EB0;
/// [static] `CPad* CPad::GetPad(int)` = 0xB73458 + n * 0x134. NewState (CControllerState) at +0.
pub const PADS: u32 = 0xB73458;

/// [static] CMenuManager (0xBA6748) fields used by WinMain state FRONTEND_IDLE (0x748CA4):
/// menu leaves -> state 8 (start game) when m_bMenuActive == 0.
pub const MENU_ACTIVATE_NEXT_FRAME: u32 = 0xBA677B;
pub const MENU_ACTIVE: u32 = 0xBA67A4;
/// [live] CTimer::m_UserPause (u8). The frontend sets it while the menu is open and clears it
/// when New Game is chosen through the menu; leaving the menu by flag writes must clear it too.
pub const TIMER_USER_PAUSE: u32 = 0xB7CB49;

/// [static] CRunningScript layout (from Init and CollectParameters).
pub const SCRIPT_SIZE: usize = 0xE0;
pub const SCRIPT_BASE_IP: usize = 0x10;
pub const SCRIPT_IP: usize = 0x14;
pub const SCRIPT_SP: usize = 0x38;
pub const SCRIPT_LOCALS: usize = 0x3C;
pub const SCRIPT_COND_RESULT: usize = 0xC5;
pub const SCRIPT_NOT_FLAG: usize = 0xD2;
pub const SCRIPT_IS_MISSION: usize = 0xDC;
pub const SCRIPT_LOCAL_COUNT: usize = 32;

/// [static] `void CWorld::Remove(CEntity*)` (cdecl)
pub const CWORLD_REMOVE: u32 = 0x563280;
/// [static] `void CWorld::Add(CEntity*)` (cdecl)
pub const CWORLD_ADD: u32 = 0x563220;

// --- globals ---
/// [static] `CPlayerInfo CWorld::Players[]`, stride 0x190; `m_pPed` at +0. Seen in FindPlayerPed.
pub const PLAYERS: u32 = 0xB7CD98;
/// CPlayerInfo::m_nMoney / m_nDisplayMoney
pub const PLAYER_MONEY: u32 = 0xB7CE50;
pub const PLAYER_DISPLAY_MONEY: u32 = 0xB7CE54;
/// CPlayerInfo::m_PlayerData.m_pWanted (CWanted*); level at CWanted+0x2C
pub const PLAYER_WANTED_PTR: u32 = 0xB7CD9C;
pub const WANTED_LEVEL: u32 = 0x2C;

/// [static] CClock::ms_nGameClockHours / Minutes (u8). Seen in CClock::Initialise.
pub const CLOCK_HOURS: u32 = 0xB70153;
pub const CLOCK_MINUTES: u32 = 0xB70152;
/// [static] CWeather::ForcedWeatherType / OldWeatherType / NewWeatherType (i16). Seen in ForceWeatherNow.
pub const WEATHER_FORCED: u32 = 0xC81318;
pub const WEATHER_OLD: u32 = 0xC81320;
pub const WEATHER_NEW: u32 = 0xC8131C;
/// [static] CTimer::m_snTimeInMilliseconds (read in CClock::Initialise)
pub const TIMER_MS: u32 = 0xB7CB84;
/// CTimer::m_FrameCounter
pub const FRAME_COUNTER: u32 = 0xB7CB4C;
/// gGameState: 5 frontend loading, 6 loaded, 7 menu, 8 loading game, 9 playing
pub const GAME_STATE: u32 = 0xC8D4C0;
pub const GS_FRONTEND_IDLE: i32 = 7;
/// CGame::currArea (interior id)
pub const CURR_AREA: u32 = 0xB72914;

/// CPools::ms_pPedPool / ms_pVehiclePool -> CPool { T* objects; u8* byteMap; i32 size; ... }
pub const PED_POOL: u32 = 0xB74490;
pub const VEHICLE_POOL: u32 = 0xB74494;
pub const PED_SIZE: u32 = 0x7C4;
pub const VEHICLE_SIZE: u32 = 0xA18;

// --- struct offsets ---
/// [static] CPlaceable: CSimpleTransform at +0x4 (pos xyz, heading f32 at +0x10), CMatrix* at +0x14.
/// Seen in CWorld::Add.
pub const ENT_PLACEMENT: u32 = 0x04;
pub const ENT_MATRIX: u32 = 0x14;
/// CMatrix: right +0x00, forward +0x10, up +0x20, pos +0x30
pub const MAT_FORWARD: u32 = 0x10;
pub const MAT_POS: u32 = 0x30;
/// CEntity::m_nModelIndex (u16), m_nAreaCode (u8)
pub const ENT_MODEL: u32 = 0x22;
pub const ENT_AREA: u32 = 0x2F;
/// CPhysical::m_vecMoveSpeed / m_vecTurnSpeed
pub const PHYS_MOVE_SPEED: u32 = 0x44;
pub const PHYS_TURN_SPEED: u32 = 0x50;
/// CPed
pub const PED_FLAGS: u32 = 0x46C; // bit 8 = bInVehicle
pub const PED_HEALTH: u32 = 0x540;
pub const PED_MAX_HEALTH: u32 = 0x544;
pub const PED_ARMOR: u32 = 0x548;
pub const PED_VEHICLE: u32 = 0x58C;
/// CVehicle
pub const VEH_DRIVER: u32 = 0x460;
pub const VEH_HEALTH: u32 = 0x4C0;
