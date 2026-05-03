use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

const MAX_FRAMES: usize = 2048;
const MAX_CHANNELS: usize = 8;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EngineParams {
    footstep_enhance: f32,
    action_detail: f32,
    gunshot_reduction: f32,
    explosion_reduction: f32,
    detection_sensitivity: f32,
    output_ceiling_db: f32,
    step_body_boost_db: f32,
    step_clarity_boost_db: f32,
    step_low_body_boost_db: f32,
    step_low_mid_boost_db: f32,
    weapon_mid_cut_db: f32,
    weapon_air_cut_db: f32,
    sustained_hold_ms: f32,
    master_duck_db: f32,
    impact_duck_db: f32,
    footstep_leveler_amount: f32,
    footstep_target_rms_db: f32,
    footstep_max_lift_db: f32,
    footstep_leveler_speed_ms: f32,
    stability_amount: f32,
    spectral_floor_db: f32,
    stable_release_ms: f32,
    footstep_guard_amount: f32,
    max_cut_step_db: f32,
    transient_kill: f32,
    lookahead_ms: f32,
    output_trim_db: f32,
    residual_reduction_db: f32,
    balance_low_db: f32,
    balance_mid_db: f32,
    balance_high_db: f32,
    stft_cutoff_hz: f32,
    stft_preserve_db: f32,
    spectral_floor_stab: f32,
    protection_pasos: f32,
    weapon_only_mode: f32,
    change_intensity: f32,
    subtlety_amount: f32,
    wet_mix: f32,
    low_shelf_freq_hz: f32,
    low_mid_freq_hz: f32,
    low_mid_q: f32,
    weapon_mid_freq_hz: f32,
    weapon_mid_q: f32,
    step_body_freq_hz: f32,
    step_body_q: f32,
    step_clarity_freq_hz: f32,
    step_clarity_q: f32,
    weapon_air_freq_hz: f32,
    weapon_air_q: f32,
    protection_attack_ms: f32,
    protection_release_ms: f32,
    boost_attack_ms: f32,
    boost_release_ms: f32,
    limiter_release_ms: f32,
    stereo_width: f32,
    protection_extreme: i32,
    spectral_mask_enabled: i32,
    debug_logging: i32,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            footstep_enhance: 65.0,
            action_detail: 45.0,
            gunshot_reduction: 85.0,
            explosion_reduction: 90.0,
            detection_sensitivity: 55.0,
            output_ceiling_db: -1.0,
            step_body_boost_db: 11.0,
            step_clarity_boost_db: 18.0,
            step_low_body_boost_db: 8.0,
            step_low_mid_boost_db: 7.0,
            weapon_mid_cut_db: -30.0,
            weapon_air_cut_db: -28.0,
            sustained_hold_ms: 0.5,
            master_duck_db: -10.0,
            impact_duck_db: -24.0,
            footstep_leveler_amount: 0.0,
            footstep_target_rms_db: -24.0,
            footstep_max_lift_db: 10.0,
            footstep_leveler_speed_ms: 80.0,
            stability_amount: 0.0,
            spectral_floor_db: -42.0,
            stable_release_ms: 220.0,
            footstep_guard_amount: 70.0,
            max_cut_step_db: 48.0,
            transient_kill: 70.0,
            lookahead_ms: 0.0,
            output_trim_db: 0.0,
            residual_reduction_db: 0.0,
            balance_low_db: 0.0,
            balance_mid_db: 0.0,
            balance_high_db: 0.0,
            stft_cutoff_hz: 2500.0,
            stft_preserve_db: 0.0,
            spectral_floor_stab: -34.0,
            protection_pasos: 85.0,
            weapon_only_mode: 0.0,
            change_intensity: 100.0,
            subtlety_amount: 35.0,
            wet_mix: 100.0,
            low_shelf_freq_hz: 250.0,
            low_mid_freq_hz: 650.0,
            low_mid_q: 0.90,
            weapon_mid_freq_hz: 1600.0,
            weapon_mid_q: 0.85,
            step_body_freq_hz: 1550.0,
            step_body_q: 1.35,
            step_clarity_freq_hz: 3500.0,
            step_clarity_q: 1.85,
            weapon_air_freq_hz: 6500.0,
            weapon_air_q: 1.00,
            protection_attack_ms: 0.5,
            protection_release_ms: 0.5,
            boost_attack_ms: 0.5,
            boost_release_ms: 0.5,
            limiter_release_ms: 0.5,
            stereo_width: 100.0,
            protection_extreme: 1,
            spectral_mask_enabled: 1,
            debug_logging: 0,
        }
    }
}

impl EngineParams {
    pub fn from_map(params: &HashMap<String, f64>) -> Self {
        let mut out = Self::default();
        out.footstep_enhance = get(params, "footstepEnhance", out.footstep_enhance);
        out.action_detail = get(params, "actionDetail", out.action_detail);
        out.gunshot_reduction = get(params, "gunshotReduction", out.gunshot_reduction);
        out.explosion_reduction = get(params, "explosionReduction", out.explosion_reduction);
        out.detection_sensitivity = get(params, "detectionSensitivity", out.detection_sensitivity);
        out.output_ceiling_db = get(params, "outputCeilingDb", out.output_ceiling_db);
        out.step_body_boost_db = get(params, "stepBodyBoostDb", out.step_body_boost_db);
        out.step_clarity_boost_db = get(params, "stepClarityBoostDb", out.step_clarity_boost_db);
        out.step_low_body_boost_db = get(params, "stepLowBodyBoostDb", out.step_low_body_boost_db);
        out.step_low_mid_boost_db = get(params, "stepLowMidBoostDb", out.step_low_mid_boost_db);
        out.weapon_mid_cut_db = get(params, "weaponMidCutDb", out.weapon_mid_cut_db);
        out.weapon_air_cut_db = get(params, "weaponAirCutDb", out.weapon_air_cut_db);
        out.sustained_hold_ms = get(params, "sustainedHoldMs", out.sustained_hold_ms);
        out.master_duck_db = get(params, "masterDuckDb", out.master_duck_db);
        out.impact_duck_db = get(params, "impactDuckDb", out.impact_duck_db);
        out.footstep_leveler_amount =
            get(params, "footstepLevelerAmount", out.footstep_leveler_amount);
        out.footstep_target_rms_db = get(params, "footstepTargetRmsDb", out.footstep_target_rms_db);
        out.footstep_max_lift_db = get(params, "footstepMaxLiftDb", out.footstep_max_lift_db);
        out.footstep_leveler_speed_ms = get(
            params,
            "footstepLevelerSpeedMs",
            out.footstep_leveler_speed_ms,
        );
        out.stability_amount = get(params, "stabilityAmount", out.stability_amount);
        out.spectral_floor_db = get(params, "spectralFloorDb", out.spectral_floor_db);
        out.stable_release_ms = get(params, "stableReleaseMs", out.stable_release_ms);
        out.footstep_guard_amount = get(params, "footstepGuardAmount", out.footstep_guard_amount);
        out.max_cut_step_db = get(params, "maxCutStepDb", out.max_cut_step_db);
        out.transient_kill = get(params, "transientKill", out.transient_kill);
        out.lookahead_ms = get(params, "lookaheadMs", out.lookahead_ms);
        out.output_trim_db = get(params, "outputTrimDb", out.output_trim_db);
        out.residual_reduction_db = get(params, "residualReductionDb", out.residual_reduction_db);
        out.balance_low_db = get(params, "balanceLowDb", out.balance_low_db);
        out.balance_mid_db = get(params, "balanceMidDb", out.balance_mid_db);
        out.balance_high_db = get(params, "balanceHighDb", out.balance_high_db);
        out.stft_cutoff_hz = get(params, "stftCutoffHz", out.stft_cutoff_hz);
        out.stft_preserve_db = get(params, "stftPreserveDb", out.stft_preserve_db);
        out.spectral_floor_stab = get(params, "spectralFloorStab", out.spectral_floor_stab);
        out.protection_pasos = get(params, "protectionPasos", out.protection_pasos);
        out.weapon_only_mode = get(params, "weaponOnlyMode", out.weapon_only_mode);
        out.change_intensity = get(params, "changeIntensity", out.change_intensity);
        out.subtlety_amount = get(params, "subtletyAmount", out.subtlety_amount);
        out.wet_mix = get(params, "wetMix", out.wet_mix);
        out.low_shelf_freq_hz = get(params, "lowShelfFreqHz", out.low_shelf_freq_hz);
        out.low_mid_freq_hz = get(params, "lowMidFreqHz", out.low_mid_freq_hz);
        out.low_mid_q = get(params, "lowMidQ", out.low_mid_q);
        out.weapon_mid_freq_hz = get(params, "weaponMidFreqHz", out.weapon_mid_freq_hz);
        out.weapon_mid_q = get(params, "weaponMidQ", out.weapon_mid_q);
        out.step_body_freq_hz = get(params, "stepBodyFreqHz", out.step_body_freq_hz);
        out.step_body_q = get(params, "stepBodyQ", out.step_body_q);
        out.step_clarity_freq_hz = get(params, "stepClarityFreqHz", out.step_clarity_freq_hz);
        out.step_clarity_q = get(params, "stepClarityQ", out.step_clarity_q);
        out.weapon_air_freq_hz = get(params, "weaponAirFreqHz", out.weapon_air_freq_hz);
        out.weapon_air_q = get(params, "weaponAirQ", out.weapon_air_q);
        out.protection_attack_ms = get(params, "protectionAttackMs", out.protection_attack_ms);
        out.protection_release_ms = get(params, "protectionReleaseMs", out.protection_release_ms);
        out.boost_attack_ms = get(params, "boostAttackMs", out.boost_attack_ms);
        out.boost_release_ms = get(params, "boostReleaseMs", out.boost_release_ms);
        out.limiter_release_ms = get(params, "limiterReleaseMs", out.limiter_release_ms);
        out.stereo_width = get(params, "stereoWidth", out.stereo_width);
        out.protection_extreme = (get(params, "protectionExtreme", 1.0) > 0.5) as i32;
        out.spectral_mask_enabled = (get(params, "spectralMaskEnabled", 1.0) > 0.5) as i32;
        out.debug_logging = (get(params, "debugLogging", 0.0) > 0.5) as i32;
        out
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct Scores {
    pub footstep: f32,
    pub action: f32,
    pub protection: f32,
    pub lateral: f32,
    pub confidence: f32,
    pub output_peak: f32,
    pub frames_analyzed: u64,
}

struct NativeDspEngine {
    ptr: NonNull<c_void>,
    enabled: AtomicBool,
}

unsafe impl Send for NativeDspEngine {}
unsafe impl Sync for NativeDspEngine {}

impl NativeDspEngine {
    fn create() -> anyhow::Result<Self> {
        let ptr = unsafe { wza_rt_create_engine() };
        let ptr = NonNull::new(ptr)
            .ok_or_else(|| anyhow::anyhow!("FirstEdition DSP engine failed to initialize"))?;
        let ok = unsafe { wza_rt_prepare_engine(ptr.as_ptr(), MAX_FRAMES, MAX_CHANNELS) };
        if ok == 0 {
            unsafe { wza_rt_destroy_engine(ptr.as_ptr()) };
            anyhow::bail!("FirstEdition DSP engine failed to prepare buffers");
        }
        Ok(Self {
            ptr,
            enabled: AtomicBool::new(false),
        })
    }

    fn set_params(&self, params: &EngineParams) {
        unsafe { wza_rt_set_params(self.ptr.as_ptr(), params as *const EngineParams) };
        self.enabled.store(true, Ordering::Release);
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        if !enabled {
            unsafe { wza_rt_reset_engine(self.ptr.as_ptr()) };
        }
    }

    fn process_interleaved_in_place(&self, samples: &mut [f32], channels: u16) {
        if !self.enabled.load(Ordering::Acquire) || channels == 0 {
            return;
        }

        let channels = channels as usize;
        let frames = samples.len() / channels;
        if frames == 0 || frames > MAX_FRAMES || channels > MAX_CHANNELS {
            return;
        }

        unsafe {
            wza_rt_process_interleaved(
                self.ptr.as_ptr(),
                samples.as_ptr(),
                samples.as_mut_ptr(),
                frames,
                channels,
            );
        }
    }

    fn scores(&self) -> Scores {
        let mut scores = Scores::default();
        unsafe { wza_rt_get_scores(self.ptr.as_ptr(), &mut scores as *mut Scores) };
        scores
    }
}

impl Drop for NativeDspEngine {
    fn drop(&mut self) {
        unsafe { wza_rt_destroy_engine(self.ptr.as_ptr()) };
    }
}

static ENGINE: OnceLock<NativeDspEngine> = OnceLock::new();
static EQAPO_NEUTRALIZED: AtomicBool = AtomicBool::new(false);

fn engine() -> anyhow::Result<&'static NativeDspEngine> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }

    let created = NativeDspEngine::create()?;
    let _ = ENGINE.set(created);
    ENGINE
        .get()
        .ok_or_else(|| anyhow::anyhow!("FirstEdition DSP engine unavailable"))
}

pub fn apply_params(params: EngineParams) -> anyhow::Result<()> {
    engine()?.set_params(&params);
    Ok(())
}

pub fn set_enabled(enabled: bool) {
    if let Ok(engine) = engine() {
        engine.set_enabled(enabled);
    }
    if !enabled {
        EQAPO_NEUTRALIZED.store(false, Ordering::Release);
    }
}

pub fn process_interleaved_in_place(samples: &mut [f32], channels: u16) {
    if let Some(engine) = ENGINE.get() {
        engine.process_interleaved_in_place(samples, channels);
    }
}

pub fn scores() -> Option<Scores> {
    ENGINE.get().map(NativeDspEngine::scores)
}

pub fn needs_eqapo_neutralization() -> bool {
    !EQAPO_NEUTRALIZED.swap(true, Ordering::AcqRel)
}

pub fn mark_eqapo_dirty() {
    EQAPO_NEUTRALIZED.store(false, Ordering::Release);
}

fn get(params: &HashMap<String, f64>, key: &str, fallback: f32) -> f32 {
    params
        .get(key)
        .copied()
        .filter(|value| value.is_finite())
        .map(|value| value as f32)
        .unwrap_or(fallback)
}

unsafe extern "C" {
    fn wza_rt_create_engine() -> *mut c_void;
    fn wza_rt_destroy_engine(engine: *mut c_void);
    fn wza_rt_prepare_engine(engine: *mut c_void, max_frames: usize, max_channels: usize) -> i32;
    fn wza_rt_reset_engine(engine: *mut c_void);
    fn wza_rt_set_params(engine: *mut c_void, params: *const EngineParams);
    fn wza_rt_process_interleaved(
        engine: *mut c_void,
        input: *const f32,
        output: *mut f32,
        frames: usize,
        channels: usize,
    );
    fn wza_rt_get_scores(engine: *mut c_void, scores: *mut Scores);
}
