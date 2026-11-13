# ai.md — Project Requirements Log

This file records every requirement given for the **soundGarden** project, so the whole project can be recreated from scratch by handing this file to an AI assistant as the prompt.

It is kept up to date as new requirements are given. Requirements are listed in the order they were given; later entries override earlier ones where they conflict (and the earlier entry is marked as
superseded).

---

## Meta / Process Requirements

1. Maintain this `ai.md` file at the repo root. Its purpose is to keep a list of all requirements so the entire project can be recreated from this file alone. Update it as work progresses, whenever a
   new requirement is given.

## Project Requirements

2. **The target sound: "the most Soundgarden guitar possible."** This is the reference for all voicing decisions:
   - Drop-D and lower or unusual tunings, with an open low string droning under the riffs.
   - Riffs in odd time signatures (7/4, 7/8, 6/4) with dissonant rubs (flat-2, tritone) against the drone. Sabbath/Melvins weight.
   - A mid-heavy tone, never scooped. Thick fuzz plus amp gain that stays articulate. Dry and close, through a 4x12-style cab.
   - Psychedelic cleaner parts through a Leslie rotating speaker ("Black Hole Sun").
   - Thayil-style leads: feedback, noise and whammy as vocabulary, not blues clichés.

3. **Build a Polyend Endless patch binary that captures that sound, using this machine's tools, and build it with `--release`.**

4. Push the project to its GitHub remote (`origin`). Sibling projects built the same way: `../nirvana` ("Smells Like Teen Spirit") and `../ironButterfly` ("In-A-Gadda-Da-Vida"). Each has its own
   `ai.md`.

5. On `master`, add the demo render (`cargo sim` output) as `samples/soundgarden_demo.wav`. Commit with the message "ai" and push.

6. Fix: in the Leslie (Black Hole) modes, notes were cut off early. The cause was the noise gate: it closed while the output was still around -45 dBFS. In the Black Hole modes the gate must sit lower
   and fade out gently so let-ring notes decay naturally. Heavy mode keeps its tight gate.

7. Keep `ai.md` lines unwrapped: only wrap where needed, or at a natural word break around 200 characters.

8. The demo's guitar sounded fake ("like MIDI"): plain Karplus-Strong noise plucks, out of tune up high, robotic timing. Make the demo guitar realistic by modelling it better (not by using samples), the same way as `../ledZeppelin` (its `src/guitar.rs`, copied here unchanged), then re-render `samples/soundgarden_demo.wav`.

### Implementation decisions (needed to recreate requirement 3)

- **Language and ABI.** Write it in `no_std` Rust (edition 2024, nightly toolchain, target `thumbv7em-none-eabihf`). The machine has no ARM GCC, so don't use the C++ SDK. Implement the FxPatchSDK
  patch ABI (version 0x000B) directly:
  - A `PatchHeader` (`'PTCH'` magic) is placed in `.patch_header` by `patch.ld`, and the firmware loads the image at 0x8000_0000. The 512 KB region holds the image plus bss.
  - The approach (`src/abi.rs`, `patch.ld`) is copied from the user's own project at `/Users/projects/endlessNam`.
- **Build settings.** `.cargo/config.toml` makes the pedal the default target, with `-C target-cpu=cortex-m7 -C target-feature=-fp64`. That way `cargo build --release` builds the patch, and no f64
  code is emitted, because the Endless FPU is single precision only.
- **Packaging and deploy.** `build-endl.sh` runs `cargo build --release`, then `llvm-objcopy -O binary` to make `target/endless/soundgarden.endl`. It fails if any `.f64` instruction or more than 512
  KB is found. `--deploy` copies the file to `/Volumes/ENDLESS` as a temp file, syncs, then renames it.
- **No libm.** sin, cos, exp2 and tanh are local f32 approximations in `src/dsp.rs`, which also holds the RBJ biquads, a one-pole filter and fractional delay lines.
- **Signal chain.** Mono in (left input), stereo out:
  1. HPF at 95 Hz.
  2. Noise gate on the input envelope (1 ms attack, 80 ms release). In Heavy it opens at 0.0012, closes at 0.0006 and fades out over 60 ms. In the Black Hole modes both thresholds are 18 dB lower and
     the fade-out is 300 ms (requirement 6).
  3. 900 Hz +5 dB peak (the Guild "honk").
  4. Fuzz stage 1: tanh with a +0.3 bias (asymmetric). Then LP 5.5 kHz and HP 70 Hz.
  5. Fuzz stage 2: tanh.
  6. Both fuzz stages use cheap 2x oversampling. Gain is 6 to 50 dB, split 60/40 between the stages.
  7. Mids EQ: 750 Hz peak.
  8. Cab: HP 75 Hz, +4 dB at 115 Hz, +3 dB at 2.4 kHz, -4 dB at 5.5 kHz, LP 4.6 kHz and LP 6.5 kHz.
  9. Leslie (see below).
  10. Level, then a tanh soft limit times 0.98.
- **Leslie.** An 800 Hz crossover splits the signal:
  - Horn: 0.83 Hz slow, 6.7 Hz fast, 40-sample Doppler, 0.55 amplitude modulation, 0.6 s inertia.
  - Drum: 0.67 Hz slow, 5.8 Hz fast, counter-rotating, 12-sample Doppler, 0.3 amplitude modulation, 3 s inertia.
  - Two virtual mics 0.6π apart give the stereo spread.
- **Feedback loop.** The fuzz output goes through a 1.25 kHz band-pass (Q 3) into a 331-sample delay, and is injected at the fuzz input. Loop gain is 1.8, normalised by the fuzz gain. It blooms over
  ~3 s while the gate is open and dies in 80 ms when muted.
- **Controls.**
  - Left knob = Fuzz (default 0.6). Mid knob = Mids (-4 to +12 dB, default 0.5). Right knob = Level (square law, unity at 0.75, default 0.75).
  - Footswitch press cycles Heavy → Black Hole (fuzz -14 dB, +9 dB makeup, slow Leslie) → Black Hole fast → Heavy. The Leslie wet mix crossfades over 0.25 s.
  - Footswitch hold toggles feedback.
  - LED colours: Heavy red / light yellow, Black Hole dim blue / light blue, fast magenta / pastel green (feedback off / on). Red also signals an error.
- **Host audition.** `cargo sim` (an alias for running on `aarch64-apple-darwin`) renders a drop-D 7/8 riff on the modelled guitar (see below), at ×0.6 to match the level the patch was voiced for, through the patch entry points in 128-sample blocks. Single low Ds are palm-muted chugs and the chords are strummed and ring; timing is nudged by up to ±6 ms, repeatably. The run covers Heavy, then Black Hole slow, then fast (ringing arpeggios), then Heavy with a sustained chord (T60 30 s, as if the loud amp keeps the strings going) and feedback on. It writes `target/render/soundgarden_demo.wav`, prints per-section peak, RMS and stereo width, and fails on NaN or full-scale output. `SG_NO_FEEDBACK=1` skips the feedback hold.

### Implementation decisions (needed to recreate requirement 8): the guitar model, `src/guitar.rs` (host only)

- **String.** Extended Karplus-Strong per note: a 4096-sample delay line read with 4-point Lagrange interpolation, so the loop delay is fractional. The loop has a one-zero loss filter `g·((1-s)·y + s·y₋₁)`, and the read delay is `SR/f - s`, so every note is in tune (measured within 0.2 cents from E2 to B5; the old integer-length string was up to ~16 cents flat).
  - Loop gain per period `g = 10^(-3 / (T60·f))`. Held T60 = `6·sqrt(82.41/f)` s, clamped to 1.8–6 s. Palm mute: T60 0.18 s, s = 0.5. Otherwise s = 0.18 + 0.12·(1 - velocity).
  - Releasing a note (the next note, unless it's allowed to ring) switches T60 to 0.06 s.
  - Two polarizations: the second is detuned +0.7 cents, decays at 0.8× the T60, and is mixed at 0.45.
- **Pick.** The excitation is one period of one-pole low-passed noise at 1.2 kHz + 6 kHz × velocity (×0.45 when palm muted), combed at the picking position (0.16 ± 0.04 of the string, random per note), with the mean removed and normalized to 0.12 × velocity RMS. Plus a click: 96 samples of differentiated noise × 0.015 × velocity, decaying over 20 samples.
  - Attack pitch: +7 cents × velocity, decaying with a 50 ms time constant.
- **Pitch movement (cents).** Vibrato 5.3 Hz, ±18 cents, fading in after 0.22 s over 0.35 s. Bend: smoothstep to the target over its duration. Slide: from N semitones below, smoothstep over its duration.
- **Pickup.** Each polarization is heard as `y - y(0.075·period)` (a bridge humbucker comb). The sum is ×0.55, then a 4.3 kHz low-pass with Q 1.3 (pickup and cable resonance) and HP 60 Hz.
- **Player.** A note can override its T60 (a fret-hand-muted "chk", or a chord sustained in front of a loud amp). Velocity varies ±12% per note, and onsets by up to 1.5 ms. Chords strum low to high (14 ms per string down, 10 ms up). The voice is dropped 0.4 s after release, or after 1.5 × T60.

9. **Buffers are zero-initialized in `.bss` and explicitly zeroed in `patch_init` before use** (rather than shipped as zeros inside the patch image). The delay lines (and reverb, where there is one) move out of the patch state into a separate `Bufs` static that is all zero at rest, so it lands in `.bss` instead of being stored as zeros in the `.endl` (runtime-length reverb combs/allpasses start with length 0 and get their lengths in init). The `Global` cell is generic; `tick` and the methods that use a delay line take `&mut Bufs`. `patch_init` then clears each buffer static in place with `core::ptr::write_bytes` (no heap or `calloc` in `no_std`, and a zeroed copy would be a stack temporary) before `init`: the loader should already have cleared `.bss`, but nothing promises it, and a NaN left in a feedback loop never decays. The firmware working buffer stays unused.

10. **Ship the built patch in a `release/` directory at the repo root** (`release/soundgarden.endl`, a copy of `target/endless/soundgarden.endl` from `./build-endl.sh`), committed to git, so a user can download it from GitHub and copy it straight to the pedal without building. README has an Install section saying so. Rebuild and recopy it whenever the patch changes.
