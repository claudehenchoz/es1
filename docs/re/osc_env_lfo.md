# Synth1 v1.13: oscillators, envelopes and LFOs (RE spec)

Target: `Synth1 VST64.dll` (sha256 51c6fe60…). Addresses are VAs (image base 0x180000000).
Tags: **[O]** = observed in code or data, **[I]** = inferred, **[U]** = unknown.
`x = v/127` (v = 0..127 sy1 value). `pow(a,b)` = FUN_180004d80. FUN_180056870/1800575e0/1800575a8/180057cc0 is the CRT log/exp fast path for pow, and FUN_180057a50 = `expf`.

## 1. Object layout and processing order

- There are 32 voices, CVoice stride 0x4a8, at synth+0x38. Inside a voice: CVco at +0x18, the filter (with filter env) at +0x1e8, CVca (amp env) at +0x3b8. [O] ctor FUN_180052370
- Param dispatch: `FUN_180028fe0(this, sy1Index, value)` loops over the 32 voices. [O]
- CVco, the filter and CVca all derive from **CEnvelopedObject** (vtable 0x1800733d0). Each one owns an ADSR. The ADSR level (an integer) is passed to the object's virtual `render(level, buf, n, lfo1val, lfo2val)` (vtable slot 7). [O] FUN_18000ca70 / FUN_18000ccc0
- Chain: `CVca.process` → child (filter) → child (CVco). The generator renders first, then the filter, then the VCA applies its gain to the buffer. [O]
- The block is split at LFO value changes and at envelope chunk boundaries. LFO values are **held constant per chunk**, and a chunk is ≥64 samples (≥32 if the LFO rate is above 10 Hz). [O] FUN_180014d10, FUN_180015210
- The two LFOs are **global** objects (synth+0xab70/0xacd0 in dispatch `this` coordinates). Every voice reads the same LFO. [O]

CVco per-block (FUN_18003dbe0 → FUN_18003bdc0 → FUN_18003bf50 → one of 72 specialised kernels at 0x18003e8f0–0x1800520b0):
```
lfoX routing (dest codes inside CVco; UI LFO dest 1..7 = osc2, osc1&2, filter, amp, PW, FM, pan):
  osc1PitchMod = porta + u90 + lfo(dest 2)            // units: 128 per octave
  osc2PitchMod = modEnv(dest osc2) + lfo(dest 1|2) + (kbdTrack ? porta + u90 : 0)
  pwMod        = modEnv(dest PW) + lfo(dest 5)
  fmMod        = modEnv(dest FM) + lfo(dest 6)
f1 = applyPitch(f1base, osc1PitchMod); f2 = applyPitch(f2base, osc2PitchMod)
applyPitch(f,m): m=clamp(m,±1280); f *= 2^(m/128) (via table fine[m&127]*coarse[m>>7]); f>=22000 → 22000   [O] FUN_18003b940
inc = (int)(f * 2^27 / sr)  ; phase is u32, one cycle = 2^27                                   [O]
pick band-limited table for f1, f2, sub (sub freq = f1/2 if sub octave on)
per sample (kernel):
  s2 = osc2(phase2)                        // raw table value (or noise)
  s1 = osc1(phase1)  (pulse: saw(φ) - saw(φ+pwOff))
  out = g1*s1 + g2*s2                       // ring on: out = s1*(g1 + g2*s2)
  if sub: out += gsub * subTable(phase1 or phase1>>1)
  phase2 += inc2 ; phase1 += inc1 + (FM ? (int)(fmDepth*s2) : 0)
  if sync and (phase1 & 0x7ffffff) wrapped: phase2 = ((oldPhase1&0x7ffffff)*inc2 mod 2^32)/inc1   // ≈0, see 2.7
```

## 2. Oscillators (CVco, vtable 0x180077308)

### 2.1 Wavetables (FUN_18003cdf0, built once) [O]
- Each table has 2048 samples. Read with `idx=(φ>>16)&2047`, `frac=(φ&0xffff)/65536`, then **linearly interpolated** (the detune kernel truncates and does not interpolate).
- Mip-levels: `f_i = 27.5·512^(i/36) = 27.5·2^(i/4)`, i=0..; `n = floor(22050/f_i)` harmonics. A level is stored while n ≥ 16, and only when n differs from the previous level. Then come levels with n = 14,13,…,1, each with `f_max = 22050/n`. 22050 is a fixed value and does not depend on the sample rate.
- Selection (FUN_18003cad0): binary search for the first level with `f_max > f`. If `f ≠ f_max` and `f ≤ 22050/14 (=1575)`, it uses the previous level (more harmonics).
- Saw: `(2/π)·Σ_{k=1..n} sin(2πkj/2048)/k`. This is a **falling** ramp, +1 at phase 0+. No sigma or Lanczos smoothing.
- Triangle: `(8/π²)·Σ_{k odd} ±sin(2πkj/2048)/k²`, with sign + for k≡1 (mod 4) and − otherwise. It is sine-phase (0 at φ=0, +1 at ¼).
- Square table: `(saw[j] − saw[j+1024])·0.5`, so ±0.5.
- Sine: `sin(2πj/2048)`. Global wave gain DAT_180094f20 = 1.0.
- Internal shape ids: 0 sine, 1 saw, 2 pulse (made from 2 saws), 3 tri, 4 noise, 5 square.

### 2.2 Shapes per parameter
| sy1 | param | mapping |
|---|---|---|
| 0 | osc1 shape 0..3 | stored raw: 0 sine, 1 saw, 2 pulse, 3 tri [O] (UI label order [U]) |
| 1 | osc2 shape 1..4 | 1 saw, 2 pulse, 3 tri, 4 noise [O] |
| 96 | sub shape 0..3 | table [sine, tri, saw, square] = ids {0,3,1,5} (DAT_1800772c8) [O] |
| 97 | sub octave 0/1 | 0 = same pitch (uses φ1), 1 = −1 oct (uses φ1>>1, bit 27) [O] |

- Pulse: `saw(φ) − saw(φ + pwOff)` with `pwOff = pw·1024` table steps (so duty = pw/2). Peak-to-peak is 2, zero-mean. [O] FUN_180051840
- Noise (osc2 only): a 1-bit LFSR at sample rate, state at CVco+0x134 (seed 10). Per sample: `if (s>>19)&1: s=((s^0x43)<<1)|1, out=+1 else s<<=1, out=−1`. It ignores pitch. [O] FUN_180051b30

### 2.3 Pitch [O]
- Note table: `note[i] = A·2^((i−69)/12)`, i=0..127, with `A = 440·2^(mastertune/1200)` (ini key "mastertune"). FUN_18003d9a0
- `f1base = note[clamp(note+keyShift,0,128)] · 2^((fine12 + uniDet)/1200)`. FUN_18003bb00
- osc2 kbd track on: `f2base = f1base · 2^(fine2/1200 + oct2)`.
  osc2 kbd track off: `f2base = 220·2^(mt/1200) · 2^((fine12+fine2+uniDet)/1200 + oct2)`. FUN_18003bba0
- `oct2` (osc2 pitch) glides linearly to its new value over **sr/20 samples (50 ms)** while a note is playing. It jumps at note-on. FUN_18003e220/FUN_18003bdc0

| sy1 | param | formula |
|---|---|---|
| 2 | osc2 pitch | `semis = floor(v·120/127) − 60` (±60 semitones, integer); oct2 = semis/12 |
| 3 | osc2 fine | cents: v<62 → v−62; 62..66 → 0; v>66 → v−66 (−62..+61 cents) |
| 72 | osc1,2 fine | same law as osc2 fine, applied to both (CVco+0x170) |
| 9 | key shift −24..24 | semitones added to note index |
| 4 | osc2 kbd track | flag 0x130 |
| 39 | portamento | `T = 2.5·x·exp(2.5(x−1))` s. Offset `(oldNote−newNote)·128/12`, decays as `off·exp(−4t)·(1−t)`, t=elapsed/T, updated in steps [O] FUN_18003dbe0 |

- `u90` (CVco+0x90) is added to both pitches. It is never written except at init (=0). Possibly pitch bend. [U]

### 2.4 Mix and gains [O] FUN_18003b8c0, FUN_18003bf50
```
m = v(sy1 5)/127 ; sub = v(sy1 95)·4/127 (0..4)
N = (sub+1)(1−m) + m
g1 = (1−m)/N ; g2 = m/N ; gsub = sub·(1−m)/N
osc1 pulse & !ring   : g1 *= 0.5
detune on & !ring    : g1 *= 0.3 ; gsub *= 0.3
osc2 pulse & FM depth==0 : g2 *= 0.5
osc2 noise & FM depth==0 : g2 *= 0.65
```
- N is also passed to CVco+0x188→+200 (FUN_18003b710). It is probably the filter drive compensation. [I]
- Mix is not smoothed. [O]

### 2.5 PW (sy1 8) [O]
- `pw0 = x` (0..1).
- PW mod pm (int, ±128 = full range): `pm≤0: pw = pw0·(1+pm/128)`; `pm>0: pw = pw0 + (2−pw0)·pm/128`.
- Clamp to 0.01..1.99, then `pwOff = (int)(pw·1024)`.
- So knob 127 gives a square wave, and knob 0 gives a ~0.5% sliver (nearly silent).

### 2.6 FM (sy1 45) [O] FUN_18003bf50 @0x18003c304
- `fm = v ± mod`: `mod>0: v + min(mod,512)·(127−v)/512`; `mod<0: v − min(−mod,512)·v/512`.
- `depth = (1000^(fm/127) − 1)/999 · 2^26` (phase units).
- FM is applied as a **phase-increment modulation of osc1 by the raw osc2 sample**: `φ1 += inc1 + (int)(depth·s2)`. At full depth this is ±½ cycle per sample.
- Depth changes are smoothed with a raised-cosine (`(cos(iπ/257)+1)/2` table DAT_1801a32e0), updated every 32 samples over ≤ sr/1000 samples. A jump larger than 2^26 is applied immediately.
- With ring on, FM kernels are not used, so there is no FM. FM also moves the sub osc, because the sub reads φ1.

### 2.7 Ring (sy1 7) and sync (sy1 6) [O]
- Ring: `out = s1·(g1 + g2·s2)`. The mix knob crossfades between osc1 and osc1×osc2.
- Sync: osc1 is master and osc2 is reset. The reset value is computed in u32 arithmetic: `((oldφ1&0x7ffffff)·inc2 mod 2^32)/inc1`. That is effectively ≈0 (under 0.05% of a cycle), so a port can reset to 0. FUN_1800486c0

### 2.8 Osc1 detune / "8 multiple" mode (sy1 76) [O] FUN_18003e470, FUN_18003bc40, FUN_180050dd0
- `det = x·20` (cents unit). The mode is active when det > 0.
- There are 9 oscillators: the centre plus 8 voices at `f1base·2^(±k·det/1200)`, k ∈ {1,3,5,7}. The outer voices are at ±7·det cents, up to ±140 cents.
- Each voice gets the osc1 pitch mod separately.
- All 9 share the centre's table and are summed **without interpolation**. Gain is g1·0.3.
- The sub is also 9-voice-summed in this mode, with gain gsub·0.3.
- FM and pulse variants exist.

### 2.9 Phase / note-on (sy1 91 phase shift) [O] FUN_18003e390, FUN_18003e600
- v=0: oscillators **free-run**. They are randomised once after the knob change, with MSVC `rand()`.
- v≥1: phases reset at every note-on. `p = (v−1)/126`:
  - φ1 = 0
  - φ2 = p·2^26, so up to **½ cycle**
  - detune voice i: `p·R[i]·2^27`
- `R[i]` (DAT_1800930d0, 512 floats) is the MSVC LCG `s=s·214013+2531011` with seed 1, `R=((s>>16)&0x7fff)/32767`.
- Unison voice u adds `uniPhase·R[10u+j]` cycles (j=0 osc1, 1 osc2, 2..9 detune).

## 3. Envelopes (CEnvelopedObject; FUN_18000ccc0, FUN_18000c780)

- Integer level. `peak = base·amount`, `sus = peak·S`. States: 1 A, 2 D, 3 S, 4 R, 0 off.
- **Attack**: linear `level = pos·peak/Alen`. On retrigger it starts from the current level (pos = Alen·level/peak). [O] FUN_18000c930
- **Decay/Release**: exponential unless the curve flag is 1 (linear). `r = 1 − pos/len`:
  `level = target + range · r · e^{8(r−1)}`, which reaches the target exactly at len. [O] @0x18000cfbe
  - Decay: range = peak−sus, target = sus.
  - Release: range = level at note-off, target 0, length is fixed (it does not scale with the start level).
- The level is re-evaluated per chunk (checkpoints at len/20, /10, /4, /3, /2). The VCA ramps gain linearly inside each chunk. If flag +0x34 is set (amp env), release ends at 0.9·len (≈−80 dB). A port can evaluate the curve per sample. [O]
- Times (sy1 12/13 mod A/D, 15/16/18 filter A/D/R, 25/26/28 amp A/D/R) [O]:
  - **A** = `0.0005·56000^x` s (0.5 ms … 28 s; x=0.5 → 0.118 s)
  - **D, R** = `0.01·6000^x` s (10 ms … 60 s)

### 3.1 Amp (CVca, vtable 0x180077100) [O] FUN_180037b00, FUN_180037c30
- base = 2048, amount 1, release-at-90% flag set, exponential curves.
- **Sustain** (27) = `v==0 ? 0 : 2048^(x^0.3)/2048`.
- **Gain** (29) uses the same law (x=1 → 1.0, x=0.5 → ≈0.24). The ctor default is 0.5.
- **Velocity** (30): `velGain = 10^(−1.5·sens·(127−vel)/127)` with sens = x, so up to −30 dB. Applied at note-on. FUN_180037bc0
- Per block: `g = level·gain·velGain/2048`. With LFO amp: `g *= (2048−l1)(2048−l2)/2^22`. g is ramped linearly from the previous block's g.
- Filter sustain (17) = x, linear (FUN_18003b780). Filter env scaling and velocity belong to the filter agent.

### 3.2 Mod env (lives inside CVco) [O] FUN_18003cba0, FUN_18003e320, FUN_18003e260
- AD only: sustain 0, release 0. Note-off does not release it, so it always runs A then D.
- On/off (sy1 10) toggles enable. When off, the level is 0.
- Amount (sy1 11): v<64 → `v/64−1`; 64 → 0; v>64 → `(v−64)/63`. A negative amount negates the level.
- Dest (sy1 71) sets base and curve:
  - 0 osc2 pitch: base 640 (= ±5 oct at amount ±1), exp decay
  - 1 FM: base 512 (= full FM sweep), **linear** decay
  - 2 PW: base 128 (= full PW mod), exp decay
- Attack steps 1 integer unit at a time (flag 0x34=0).

## 4. LFO (CLFO, vtable 0x180073d78; FUN_180014d10 value, FUN_180015080 depth, FUN_180015210 rate)

- Waves (sy1 42/47, stored raw) [O]:
  - 0 saw, falling max→min
  - 1 triangle: min→max over the first half, then down
  - 2 square: max for the first half, min for the second
  - 3 S&H: new `rand()%range` each cycle
  - 4 smooth random: linear from the previous random to the new random over the cycle
  - 5 sine, using the 2048 sine table: `((max−min)·sin + max+min)/2`
  (UI label order [U])
- **Rate** (43/48), free: `f = 1500^x / 12` Hz (0.083 … 125 Hz). The rate slews towards its target at 1 Hz per ms. [O] FUN_180014ff0
- **Tempo sync** (67/69): `f = 0.4·bpm / T[(v·30)>>7]`, with T in 24-ppq ticks (96 = 1 bar):
  `T = {1536,1152,864,768,576,432,384,288,216,192,144,96,84,72,48,42,36,32,24,21,18,16,12,9,8,6,4,3,2,1}` (DAT_180073cf0) [O]
- **Key sync** (68/70): on every note-on, phase=0 and a new random target. The LFO is global, so this resets it for all voices. [O] @0x18002807d
- **Depth** (44/49): `d = (10^x−1)/9` for pitch dests (1,2), otherwise `d = x`. Output range in modulation units [O]:

| dest | range | meaning |
|---|---|---|
| 1 osc2 / 2 osc1&2 pitch | ±640·d | 128/oct, so ±5 oct |
| 3 filter | 0..1024·d | (filter agent) |
| 4 amp | 0..2048·d | inverted min/max; VCA multiplies by (2048−l)/2048 |
| 5 PW | ±128·d | |
| 6 FM | 0..512·d | |
| 7 pan | ±64·d | |

- Enable (57/58) is LFO+8. Voices only route dest {1,2,5,6} to CVco and 4 to CVca. [O] FUN_18003bd40, FUN_180037b90

## 5. Gaps
- [U] Writer of CVco+0x90 (pitch offset for both oscillators; pitch bend?).
- [U] UI ordering versus internal ids for the osc and LFO waves.
- [U] The object at CVco+0x188, which receives the mix normaliser N.
- [U] FM-kernel noise behaviour was not decompiled.
- Unison detune law (FUN_180036460) and the filter env and velocity mapping were not examined.
