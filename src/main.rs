//! soundGarden: a Polyend Endless patch that aims for the most Soundgarden
//! guitar possible.
//!
//! Signal chain (mono in from the left input, stereo out):
//!   tight HPF -> noise gate -> honky pre-mid push -> 2-stage asymmetric fuzz
//!   -> Mids EQ (never scooped) -> 4x12 cab sim -> Leslie rotary -> level
//!   with a controlled-feedback loop around the fuzz for Thayil-style howl.
//!
//! Controls
//!   Left knob   Fuzz   (gain into the fuzz; ~6 to 50 dB)
//!   Mid knob    Mids   (750 Hz peak, -4 to +12 dB; noon = +4 dB, mid-heavy)
//!   Right knob  Level  (output; unity at 3 o'clock)
//!   Footswitch press  cycle mode: Heavy -> Black Hole (slow rotary) -> Black Hole fast -> Heavy
//!   Footswitch hold   feedback on/off (sustained notes bloom into howling feedback)
//!   LED  Heavy: red | Black Hole slow: dim blue | Black Hole fast: magenta
//!        with feedback on: light yellow | light blue | pastel green
//!
//! Host builds (`cargo sim`) render a demo riff through the same code; see `host`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod abi;
mod dsp;

use core::cell::UnsafeCell;
use core::f32::consts::TAU;

use abi::*;
use dsp::*;

// ---------------------------------------------------------------- voicing

/// Modes, cycled by footswitch press.
const HEAVY: u8 = 0;
const BLACK_HOLE: u8 = 1;
const BLACK_HOLE_FAST: u8 = 2;
const MODES: u8 = 3;

/// Fuzz gain (dB) at knob 0 and the extra at knob 1, split across two stages.
const FUZZ_MIN_DB: f32 = 6.0;
const FUZZ_RANGE_DB: f32 = 44.0;
/// Black Hole modes clean up the fuzz this much (dB) for the psychedelic parts.
const BLACK_HOLE_FUZZ_CUT_DB: f32 = 14.0;
/// Asymmetric bias of the first clipper (even harmonics, "sputter").
const ASYM_BIAS: f32 = 0.3;

/// Noise gate thresholds on the input envelope (linear), with hysteresis.
const GATE_OPEN: f32 = 0.0012;
const GATE_CLOSE: f32 = 0.0006;

/// Feedback loop: delay (samples, ~ distance amp -> guitar), band-pass centre
/// and Q (where the howl blooms), and loop gain at full bloom.
const FB_DELAY: f32 = 331.0;
const FB_FREQ: f32 = 1250.0;
const FB_Q: f32 = 3.0;
const FB_LOOP: f32 = 1.8;
/// Seconds for feedback to bloom while a note sustains, and to die when muted.
const FB_BLOOM_S: f32 = 3.0;
const FB_DIE_S: f32 = 0.08;

/// Leslie: rotor speeds (Hz), speed-change inertia (s), crossover, Doppler
/// depth / base delay (samples), amplitude modulation depth.
const HORN_SLOW: f32 = 0.83;
const HORN_FAST: f32 = 6.7;
const DRUM_SLOW: f32 = 0.67;
const DRUM_FAST: f32 = 5.8;
const HORN_INERTIA_S: f32 = 0.6;
const DRUM_INERTIA_S: f32 = 3.0;
const XOVER_HZ: f32 = 800.0;
const HORN_DOPPLER: f32 = 40.0;
const DRUM_DOPPLER: f32 = 12.0;
const ROT_BASE: f32 = 4.0;
const HORN_AM: f32 = 0.55;
const DRUM_AM: f32 = 0.3;
/// Right mic angle relative to the left mic (radians).
const MIC_R_ANGLE: f32 = 0.6 * core::f32::consts::PI;

/// Output trim: unity Level puts the Heavy riff around -14 dB RMS.
const OUT_TRIM: f32 = 0.27;
/// Black Hole modes run less fuzz, so they get this much output makeup (dB).
const BLACK_HOLE_MAKEUP_DB: f32 = 9.0;

// ---------------------------------------------------------------- state

struct Sg {
    ready: bool,
    error: bool,
    // knobs, 0..1
    fuzz: f32,
    mids: f32,
    level: f32,
    mode: u8,
    feedback: bool,
    // smoothed gains and their targets
    g1: f32,
    g2: f32,
    out_g: f32,
    g1_t: f32,
    g2_t: f32,
    out_t: f32,
    wet: f32,
    fb_ramp: f32,
    // filters
    hp_in: Biquad,
    pre_mid: Biquad,
    inter_lp: Biquad,
    inter_hp: Biquad,
    mid_eq: Biquad,
    cab: [Biquad; 6],
    fb_bp: Biquad,
    u1: f32,
    u2: f32,
    // gate
    env: f32,
    gate: f32,
    gate_open: bool,
    fb_line: Delay<512>,
    // rotary
    xover: OnePole,
    horn: Delay<128>,
    drum: Delay<64>,
    horn_ph: f32,
    drum_ph: f32,
    horn_hz: f32,
    drum_hz: f32,
    mic_r: (f32, f32),
    // coefficients
    c_param: f32,
    c_wet: f32,
    c_horn: f32,
    c_drum: f32,
    c_att: f32,
    c_rel: f32,
    c_gate_open: f32,
    c_gate_close: f32,
    c_fb_up: f32,
    c_fb_down: f32,
}

impl Sg {
    const NEW: Self = Self {
        ready: false,
        error: false,
        fuzz: 0.6,
        mids: 0.5,
        level: 0.75,
        mode: HEAVY,
        feedback: false,
        g1: 1.0,
        g2: 1.0,
        out_g: 0.0,
        g1_t: 1.0,
        g2_t: 1.0,
        out_t: 0.0,
        wet: 0.0,
        fb_ramp: 0.0,
        hp_in: Biquad::NEW,
        pre_mid: Biquad::NEW,
        inter_lp: Biquad::NEW,
        inter_hp: Biquad::NEW,
        mid_eq: Biquad::NEW,
        cab: [Biquad::NEW; 6],
        fb_bp: Biquad::NEW,
        u1: 0.0,
        u2: 0.0,
        env: 0.0,
        gate: 0.0,
        gate_open: false,
        fb_line: Delay::NEW,
        xover: OnePole::NEW,
        horn: Delay::NEW,
        drum: Delay::NEW,
        horn_ph: 0.0,
        drum_ph: 0.0,
        horn_hz: HORN_SLOW,
        drum_hz: DRUM_SLOW,
        mic_r: (1.0, 0.0),
        c_param: 1.0,
        c_wet: 1.0,
        c_horn: 1.0,
        c_drum: 1.0,
        c_att: 1.0,
        c_rel: 1.0,
        c_gate_open: 1.0,
        c_gate_close: 1.0,
        c_fb_up: 1.0,
        c_fb_down: 1.0,
    };

    fn init(&mut self) {
        self.hp_in.highpass(95.0, 0.707); // tight lows into the fuzz
        self.pre_mid.peak(900.0, 0.7, 5.0); // Guild-ish honk
        self.inter_lp.lowpass(5500.0, 0.707);
        self.inter_hp.highpass(70.0, 0.707); // also removes the asym clipper's DC
        // 4x12 with greenback/V30-style voicing: thump, presence, steep top roll-off.
        self.cab[0].highpass(75.0, 0.707);
        self.cab[1].peak(115.0, 1.4, 4.0);
        self.cab[2].peak(2400.0, 1.8, 3.0);
        self.cab[3].peak(5500.0, 2.0, -4.0);
        self.cab[4].lowpass(4600.0, 0.8);
        self.cab[5].lowpass(6500.0, 0.6);
        self.fb_bp.bandpass(FB_FREQ, FB_Q);
        self.xover.set(XOVER_HZ);
        self.mic_r = (cos(MIC_R_ANGLE), sin(MIC_R_ANGLE));
        self.c_param = coef(0.02);
        self.c_wet = coef(0.25);
        self.c_horn = coef(HORN_INERTIA_S);
        self.c_drum = coef(DRUM_INERTIA_S);
        self.c_att = coef(0.001);
        self.c_rel = coef(0.08);
        self.c_gate_open = coef(0.002);
        self.c_gate_close = coef(0.06);
        self.c_fb_up = coef(FB_BLOOM_S / 3.0); // ~95% bloomed after FB_BLOOM_S
        self.c_fb_down = coef(FB_DIE_S);
        self.retarget();
        self.g1 = self.g1_t;
        self.g2 = self.g2_t;
        self.out_g = self.out_t;
        self.ready = true;
    }

    /// Recompute gain targets and the Mids EQ from the knobs and mode.
    fn retarget(&mut self) {
        let mut total = FUZZ_MIN_DB + FUZZ_RANGE_DB * self.fuzz;
        if self.mode != HEAVY {
            total -= BLACK_HOLE_FUZZ_CUT_DB;
        }
        self.g1_t = db(total * 0.6);
        self.g2_t = db(total * 0.4);
        // Square law: 0 at minimum, unity at 3 o'clock, +5 dB at maximum.
        let v = self.level / 0.75;
        self.out_t = v * v * OUT_TRIM * if self.mode == HEAVY { 1.0 } else { db(BLACK_HOLE_MAKEUP_DB) };
        self.mid_eq.peak(750.0, 0.8, -4.0 + 16.0 * self.mids);
    }

    fn rotary(&mut self, y: f32) -> (f32, f32) {
        let fast = self.mode == BLACK_HOLE_FAST;
        let (ht, dt) = if fast { (HORN_FAST, DRUM_FAST) } else { (HORN_SLOW, DRUM_SLOW) };
        self.horn_hz += (ht - self.horn_hz) * self.c_horn;
        self.drum_hz += (dt - self.drum_hz) * self.c_drum;
        // Horn and drum turn in opposite directions.
        self.horn_ph += TAU * self.horn_hz / SR;
        if self.horn_ph > TAU {
            self.horn_ph -= TAU;
        }
        self.drum_ph -= TAU * self.drum_hz / SR;
        if self.drum_ph < 0.0 {
            self.drum_ph += TAU;
        }
        let lo = self.xover.process(y);
        self.horn.push(y - lo);
        self.drum.push(lo);
        let (hc, hs) = (cos(self.horn_ph), sin(self.horn_ph));
        let (dc, ds) = (cos(self.drum_ph), sin(self.drum_ph));
        let (mc, ms) = self.mic_r;
        // facing = cos(rotor - mic) in [-1, 1]: 1 when the rotor points at the mic.
        let l = tap(&self.horn, hc, HORN_DOPPLER, HORN_AM) + tap(&self.drum, dc, DRUM_DOPPLER, DRUM_AM);
        let r = tap(&self.horn, hc * mc + hs * ms, HORN_DOPPLER, HORN_AM)
            + tap(&self.drum, dc * mc + ds * ms, DRUM_DOPPLER, DRUM_AM);
        (l, r)
    }

    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        self.g1 += (self.g1_t - self.g1) * self.c_param;
        self.g2 += (self.g2_t - self.g2) * self.c_param;
        self.out_g += (self.out_t - self.out_g) * self.c_param;
        let wet_t = if self.mode == HEAVY { 0.0 } else { 1.0 };
        self.wet += (wet_t - self.wet) * self.c_wet;

        let x = self.hp_in.process(x);

        // Gate on the input envelope.
        let a = x.abs();
        self.env += (a - self.env) * if a > self.env { self.c_att } else { self.c_rel };
        if self.env > GATE_OPEN {
            self.gate_open = true;
        } else if self.env < GATE_CLOSE {
            self.gate_open = false;
        }
        let (gt, gc) = if self.gate_open { (1.0, self.c_gate_open) } else { (0.0, self.c_gate_close) };
        self.gate += (gt - self.gate) * gc;

        // Feedback blooms while a note sustains and dies as soon as it's muted.
        // Injection is normalised by the fuzz gain so the loop gain (and so
        // the bloom) is the same at any Fuzz setting.
        let ft = if self.feedback && self.gate_open { 1.0 } else { 0.0 };
        self.fb_ramp += (ft - self.fb_ramp) * if ft > self.fb_ramp { self.c_fb_up } else { self.c_fb_down };
        let fb = self.fb_line.read(FB_DELAY) * self.fb_ramp * FB_LOOP / (self.g1 * self.g2);

        // Two fuzz stages, each with cheap 2x oversampling (linear-interpolated
        // midpoint, averaged back down).
        let u = self.pre_mid.process(x + fb) * self.g1;
        let y = 0.5 * (asym(0.5 * (u + self.u1)) + asym(u));
        self.u1 = u;
        let y = self.inter_hp.process(self.inter_lp.process(y));
        let u = y * self.g2;
        let y = 0.5 * (tanh(0.5 * (u + self.u2)) + tanh(u));
        self.u2 = u;
        let y = y * self.gate;
        self.fb_line.push(self.fb_bp.process(y));

        let mut y = self.mid_eq.process(y);
        for b in self.cab.iter_mut() {
            y = b.process(y);
        }

        let (rl, rr) = self.rotary(y);
        let l = (y + (rl - y) * self.wet) * self.out_g;
        let r = (y + (rr - y) * self.wet) * self.out_g;
        (limit(l), limit(r))
    }

    fn led(&self) -> Color {
        if self.error {
            return Color::Red;
        }
        match (self.mode, self.feedback) {
            (HEAVY, false) => Color::Red,
            (HEAVY, true) => Color::LightYellow,
            (BLACK_HOLE, false) => Color::DimBlue,
            (BLACK_HOLE, true) => Color::LightBlue,
            (_, false) => Color::Magenta,
            (_, true) => Color::PastelGreen,
        }
    }
}

/// First fuzz stage: biased tanh, re-centred so silence stays silent.
#[inline]
fn asym(u: f32) -> f32 {
    tanh(u + ASYM_BIAS) - tanh(ASYM_BIAS)
}

/// One rotor tap: Doppler delay and loudness follow how directly it faces the mic.
#[inline]
fn tap<const N: usize>(d: &Delay<N>, facing: f32, doppler: f32, am: f32) -> f32 {
    let away = (1.0 - facing) * 0.5; // 0 facing the mic .. 1 facing away
    d.read(ROT_BASE + doppler * away) * (1.0 - am * away)
}

/// Output soft limiter, never reaching ±1.
#[inline]
fn limit(x: f32) -> f32 {
    tanh(x) * 0.98
}

/// All patch state. The firmware calls every entry point from the audio
/// thread (SDK: "no need to synchronize"). Lives in .bss/.data.
struct Global(UnsafeCell<Sg>);
unsafe impl Sync for Global {}
static STATE: Global = Global(UnsafeCell::new(Sg::NEW));

#[allow(clippy::mut_from_ref)]
fn state() -> &'static mut Sg {
    // SAFETY: single-threaded access from the firmware's audio thread.
    unsafe { &mut *STATE.0.get() }
}

// ---------------------------------------------------------------- entry points

const PARAM_FUZZ: i32 = 0;
const PARAM_MIDS: i32 = 1;
const PARAM_LEVEL: i32 = 2;

unsafe extern "C" fn patch_init(env: *const PatchEnv) {
    if env.is_null() || unsafe { (*env).abi_version } != PATCH_ABI_VERSION as u32 {
        return;
    }
    state().init();
}

unsafe extern "C" fn update_buffers(_: *const PatchEnv, left: *mut f32, right: *mut f32, samples: i32) -> f32 {
    let Ok(n) = usize::try_from(samples) else { return 0.0 };
    if left.is_null() || right.is_null() || n == 0 {
        return 0.0;
    }
    // SAFETY: the firmware passes two buffers of `samples` floats.
    let (l, r) = unsafe { (core::slice::from_raw_parts_mut(left, n), core::slice::from_raw_parts_mut(right, n)) };
    let s = state();
    if !s.ready {
        return 0.0;
    }
    for (a, b) in l.iter_mut().zip(r.iter_mut()) {
        let (ol, or) = s.tick(*a);
        *a = ol;
        *b = or;
    }
    0.0
}

unsafe extern "C" fn set_buffer(_: *const PatchEnv, buffer: *mut f32) {
    // All state is static; the working buffer isn't needed.
    if buffer.is_null() {
        state().error = true;
    }
}

unsafe extern "C" fn get_buffer_size(_: *const PatchEnv) -> i32 {
    WORKING_BUFFER_SIZE
}

unsafe extern "C" fn get_param_min(_: *const PatchEnv, _: i32) -> f32 {
    0.0
}

unsafe extern "C" fn get_param_max(_: *const PatchEnv, _: i32) -> f32 {
    1.0
}

unsafe extern "C" fn get_param_default(_: *const PatchEnv, idx: i32) -> f32 {
    match idx {
        PARAM_FUZZ => 0.6,
        PARAM_LEVEL => 0.75,
        _ => 0.5,
    }
}

unsafe extern "C" fn is_param_enabled(_: *const PatchEnv, idx: i32, source_id: i32) -> u8 {
    // Knobs only (source 0), like the SDK wrapper.
    (source_id == 0 && (0..PARAM_COUNT).contains(&idx)) as u8
}

unsafe fn write_c_str(s: &str, out: *mut u8, len: usize) {
    if out.is_null() || len == 0 {
        return;
    }
    let n = s.len().min(len - 1);
    // SAFETY: the firmware provides `len` writable bytes at `out`.
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), out, n);
        *out.add(n) = 0;
    }
}

unsafe extern "C" fn get_param_name(_: *const PatchEnv, idx: i32, out: *mut u8, len: usize) {
    let name = match idx {
        PARAM_FUZZ => "Fuzz",
        PARAM_MIDS => "Mids",
        PARAM_LEVEL => "Level",
        _ => "",
    };
    unsafe { write_c_str(name, out, len) }
}

unsafe extern "C" fn get_param_unit(_: *const PatchEnv, _: i32, out: *mut u8, len: usize) {
    unsafe { write_c_str("", out, len) }
}

unsafe extern "C" fn set_param(_: *const PatchEnv, idx: i32, value: f32) {
    let s = state();
    let v = value.clamp(0.0, 1.0);
    match idx {
        PARAM_FUZZ => s.fuzz = v,
        PARAM_MIDS => s.mids = v,
        PARAM_LEVEL => s.level = v,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn special_action(_: *const PatchEnv, idx: i32) {
    let s = state();
    match idx {
        ACTION_FOOTSWITCH_PRESS => s.mode = (s.mode + 1) % MODES,
        ACTION_FOOTSWITCH_HOLD => s.feedback = !s.feedback,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn get_state_idx(_: *const PatchEnv) -> i32 {
    state().led() as i32
}

// ---------------------------------------------------------------- header

#[cfg(target_os = "none")]
unsafe extern "C" {
    static __patch_bss_start: u8;
    static patch_image_size: u8;
    static patch_bss_size: u8;
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".patch_header")]
#[used]
pub static patch_header: PatchHeader = PatchHeader {
    magic: PATCH_MAGIC,
    abi_version: PATCH_ABI_VERSION,
    flags: PATCH_FLAG_NONE,
    init: patch_init,
    agent_update_buffers: update_buffers,
    agent_set_buffer: set_buffer,
    agent_get_buffer_size: get_buffer_size,
    agent_get_param_min: get_param_min,
    agent_get_param_max: get_param_max,
    agent_get_param_default: get_param_default,
    agent_is_param_enabled: is_param_enabled,
    agent_get_param_name: get_param_name,
    agent_get_param_unit: get_param_unit,
    agent_set_param: set_param,
    agent_special_action: special_action,
    agent_get_state_idx: get_state_idx,
    image_size: &raw const patch_image_size,
    bss_begin: &raw const __patch_bss_start,
    bss_size: &raw const patch_bss_size,
    reserved: [0; 16],
};

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(target_os = "none"))]
mod host;

#[cfg(not(target_os = "none"))]
fn main() {
    host::main();
}
