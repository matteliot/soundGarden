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
- **Host audition.** `cargo sim` (an alias for running on `aarch64-apple-darwin`) renders a Karplus-Strong drop-D 7/8 riff through the patch entry points in 128-sample blocks. The run covers Heavy,
  then Black Hole slow, then fast, then Heavy with a sustained chord and feedback on. It writes `target/render/soundgarden_demo.wav`, prints per-section peak, RMS and stereo width, and fails on NaN or
  full-scale output. `SG_NO_FEEDBACK=1` skips the feedback hold.
