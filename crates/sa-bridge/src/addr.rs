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
/// gGameState (9 = playing)
pub const GAME_STATE: u32 = 0xC8D4C0;
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
