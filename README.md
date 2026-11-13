# soundGarden

A [Polyend Endless](https://polyend.com/endless/) patch that tries to be the most
Soundgarden guitar possible: drop-tuned, mid-heavy fuzz through a 4x12, a Leslie
for the *Black Hole Sun* parts, and a controlled-feedback loop for Thayil-style
howl. It's written in `no_std` Rust against the patch ABI of Polyend's
[FxPatchSDK](https://github.com/polyend/FxPatchSDK) (ABI 0x000B), so no ARM GCC is needed.

Signal chain (mono in from the left input, stereo out):
`HPF 95 Hz → gate → 900 Hz push → asymmetric fuzz → fuzz → Mids EQ → 4x12 cab → Leslie → level`
plus a feedback loop (1.25 kHz band-pass, 7 ms delay) from the fuzz output back to its input.

| Control | Function |
|---|---|
| Left knob | **Fuzz**: gain into the two fuzz stages, about 6 to 50 dB |
| Mid knob | **Mids**: 750 Hz peak, -4 to +12 dB (noon = +4 dB: mid-heavy, never scooped) |
| Right knob | **Level**: output, unity at 3 o'clock |
| Footswitch press | Mode: **Heavy** → **Black Hole** (less fuzz, slow Leslie) → **Black Hole fast** → Heavy |
| Footswitch hold | **Feedback** on/off: sustained notes bloom into howl over ~3 s and die when muted |

| LED | Feedback off | Feedback on |
|---|---|---|
| Heavy | red | light yellow |
| Black Hole (slow) | dim blue | light blue |
| Black Hole fast | magenta | pastel green |

The Leslie speeds up and slows down with rotor inertia (horn ~0.6 s, drum ~3 s),
so switching between slow and fast ramps like the real thing.

## Build

```sh
rustup target add thumbv7em-none-eabihf --toolchain nightly
rustup component add llvm-tools --toolchain nightly
cargo build --release        # ELF for the pedal (default target is thumbv7em-none-eabihf)
./build-endl.sh              # build + make target/endless/soundgarden.endl, check f64 use and size
./build-endl.sh --deploy     # ...and copy to /Volumes/ENDLESS
```

## Audition on your computer

```sh
cargo sim                    # -> target/render/soundgarden_demo.wav
```

This renders a synthesized drop-D riff in 7/8 on a modelled electric guitar (`src/guitar.rs`: exact tuning, pick and humbucker, palm mutes, strums, vibrato and human timing) through the same patch code. It
goes through Heavy, then Black Hole slow, then Black Hole fast, then Heavy with
a sustained chord and feedback on. Then it prints the level of each section.
