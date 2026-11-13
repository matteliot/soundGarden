//! Host-only demo renderer: `cargo sim [OUT.wav]`.
//!
//! Plays a drop-D riff in 7/8 on a modelled electric guitar (see `guitar`:
//! exact tuning, pick and pickup, palm mutes, strums, human timing), drives
//! the patch's entry points the way the firmware does (128-sample blocks,
//! footswitch presses/holds at set times) and writes a stereo float WAV.
//! Prints per-section level stats and fails on NaN or clipping.

use crate::guitar::{Note, Player};
use crate::*;

const N: usize = 128;
const SRU: usize = 48_000;
/// The modelled guitar into this patch, matched to the level the patch was voiced for.
const INPUT_TRIM: f32 = 0.6;

// Drop D, a few dissonant neighbours.
const D2: f32 = 73.42;
const A2: f32 = 110.0;
const C3: f32 = 130.81;
const CS3: f32 = 138.59;
const D3: f32 = 146.83;
const EB3: f32 = 155.56;
const F3: f32 = 174.61;
const GS3: f32 = 207.65;
const A3: f32 = 220.0;
const D4: f32 = 293.66;
const F4: f32 = 349.23;

enum Ev {
    /// Play these notes, strummed this many seconds apart; `ring` lets what's already sounding carry on.
    Play(Vec<Note>, f32, bool),
    Press,
    Hold,
}

fn score() -> (Vec<(f64, Ev, &'static str)>, f64) {
    let mut ev = vec![];
    let eighth = 0.2;
    // Small, repeatable timing slop: nobody plays exactly on the grid.
    let mut seed: u32 = 0x1357_9bdf;
    let mut slop = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) * 0.006
    };
    // Heavy: 4 bars of 7/8 over an open low-D drone, flat-2 and tritone rubs. Single low Ds are palm-muted chugs; the chords ring.
    let bar: [&'static [f32]; 7] = [&[D2, A2, D3], &[D2], &[D2], &[D2, EB3], &[D2], &[D2, GS3], &[C3, F3]];
    let mut t = 0.1;
    for _ in 0..4 {
        for (k, c) in bar.into_iter().enumerate() {
            let notes: Vec<Note> = if c.len() == 1 { vec![Note::new(c[0]).pm()] } else { c.iter().map(|f| Note::new(*f).vel(if k == 0 { 0.95 } else { 0.85 })).collect() };
            ev.push((t + slop(), Ev::Play(notes, 0.009, false), ""));
            t += eighth;
        }
    }
    // Black Hole slow, then fast: ringing arpeggios sliding down chromatically.
    for label in ["black hole (slow rotary)", "black hole fast"] {
        ev.push((t, Ev::Press, label));
        t += 0.1;
        for root in [D3, CS3, C3] {
            for (k, f) in [root, A3, D4, F4].into_iter().enumerate() {
                ev.push((t + slop(), Ev::Play(vec![Note::new(f).vel(0.7)], 0.0, k != 0), ""));
                t += 0.33;
            }
        }
        t += 0.3;
    }
    ev.push((t, Ev::Press, "heavy + feedback (sustained D)"));
    if std::env::var_os("SG_NO_FEEDBACK").is_none() {
        ev.push((t + 0.01, Ev::Hold, ""));
    }
    // Held in front of a loud amp, the strings keep ringing (the amp drives them), so this chord sustains far longer than a plucked one would.
    ev.push((t + 0.1, Ev::Play([D2, A2, D3].iter().map(|f| Note::new(*f).vel(0.95).t60(30.0)).collect(), 0.012, false), ""));
    t += 6.0;
    (ev, t)
}

pub fn main() {
    let out = std::env::args().nth(1).unwrap_or("target/render/soundgarden_demo.wav".into());
    let env = PatchEnv { abi_version: PATCH_ABI_VERSION as u32, user_ctx: core::ptr::null_mut() };
    let mut work = vec![0.0f32; 16];
    unsafe {
        patch_init(&env);
        set_buffer(&env, work.as_mut_ptr());
        for i in 0..PARAM_COUNT {
            set_param(&env, i, get_param_default(&env, i));
        }
    }

    let (mut events, total_s) = score();
    events.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total = (total_s * SRU as f64) as usize;
    let mut guitar = Player::new();

    let mut lo = vec![];
    let mut ro = vec![];
    let mut sections: Vec<(usize, &str)> = vec![(0, "heavy (drop-D riff in 7/8)")];
    let mut t = 0;
    while t < total {
        while events.last().is_some_and(|(et, _, _)| (*et * SRU as f64) as usize <= t) {
            let (_, e, label) = events.pop().unwrap();
            match e {
                Ev::Press => unsafe { special_action(&env, ACTION_FOOTSWITCH_PRESS) },
                Ev::Hold => unsafe { special_action(&env, ACTION_FOOTSWITCH_HOLD) },
                Ev::Play(notes, strum, ring) => guitar.play(&notes, strum, ring),
            }
            if !label.is_empty() {
                sections.push((t, label));
            }
        }
        let mut l = [0.0f32; N];
        let mut r = [0.0f32; N];
        for x in l.iter_mut() {
            *x = guitar.tick() * INPUT_TRIM;
        }
        unsafe { update_buffers(&env, l.as_mut_ptr(), r.as_mut_ptr(), N as i32) };
        lo.extend_from_slice(&l);
        ro.extend_from_slice(&r);
        t += N;
    }

    let mut bad = false;
    sections.push((lo.len(), ""));
    println!("{:<34} {:>8} {:>8} {:>10}", "section", "peak", "rms dB", "L-R dB");
    for w in sections.windows(2) {
        let (a, name) = w[0];
        let b = w[1].0;
        let (l, r) = (&lo[a..b], &ro[a..b]);
        let peak = l.iter().chain(r).fold(0.0f32, |m, v| m.max(v.abs()));
        let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
        let side: Vec<f32> = l.iter().zip(r).map(|(a, b)| a - b).collect();
        let to_db = |v: f64| 20.0 * v.max(1e-9).log10();
        println!("{name:<34} {peak:>8.3} {:>8.1} {:>10.1}", to_db(rms(l)), to_db(rms(&side)));
        if name.starts_with("heavy + feedback") {
            // Bloom: compare the first and last second of the sustained note.
            let s = SRU;
            let early = rms(&l[s / 2..s / 2 + s]);
            let late = rms(&l[l.len() - 2 * s..l.len() - s]);
            println!("  feedback bloom: {:.1} dB (1 s in) -> {:.1} dB (4 s in)", to_db(early), to_db(late));
        }
        if l.iter().chain(r).any(|v| !v.is_finite()) || peak >= 1.0 {
            bad = true;
        }
    }
    let led = unsafe { get_state_idx(&env) };
    println!("final LED colour index: {led}");

    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    write_wav(&out, &lo, &ro);
    println!("wrote {out} ({:.1} s)", lo.len() as f64 / SRU as f64);
    if bad {
        eprintln!("error: NaN or full-scale output");
        std::process::exit(1);
    }
    let _ = (get_buffer_size, get_param_min, get_param_max, is_param_enabled, get_param_name, get_param_unit);
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let bytes = (l.len() * 8) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + bytes).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&bytes.to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}
