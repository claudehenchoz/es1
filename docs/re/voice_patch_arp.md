# Synth1 v1.13: patch application, voice management, arpeggiator, master gain

Target: `Synth1 VST64.dll` (image base 0x180000000). Addresses are `FUN_` entry points.
Tags: **[O]** observed in code, **[I]** inferred, **[U]** unknown.
`x = v/127` everywhere unless stated. `T` = the built-in "initial sound" template.
Helpers: `FUN_180004d80(a,b)` = pow(a,b) (exp(log a * b), with a CRT pow fallback), `FUN_1800575e0` = exp,
`FUN_180056870` = log, `FUN_180057a50` = expf, `FUN_18005b840` = logf, `FUN_180057cc0` = CRT pow [O].

---------------------------------------------------------------------------------------------------
## 1. Patch to parameter application (TOP PRIORITY)

### 1.1 Data structures [O]
`CSoundData` (ctor `FUN_18001c1e0`, size 0x564):
`+0x08 name[32]`, `+0x28 color`, `+0x2c int idx[100]` (-1 = empty), `+0x1bc int val[100]`, `+0x34c count`,
`+0x350 bank`, `+0x354 path[...]`, `+0x55c program`, `+0x560 version`.
Lookup `FUN_18001dea0(sd, idx)` returns 0 when the idx is not present. Set/insert is `FUN_18001e9c0`, and entry i is `FUN_18001d600`.

The CSynthesizer1 (`this`) offsets that matter here: `+0xae38` loader (contains a CSoundData at +0x210),
`+0xb5b0` the template CSoundData **T**, `+0xbb10` T.version = 113, `+0xbb18` CSoundData* current patch.

### 1.2 Where the defaults come from [O]
The `CSynthesizer1` ctor `FUN_180025110` names T "initial sound" (+0xb5b8), sets version 0x71 = **113**, and inserts
**99 (idx,val) pairs from the table at 0x180093af0** (loop `uVar12 < 0xc6`, step 2). The table covers each sy1 index 0..98 exactly
once. Then it calls `FUN_180027c70(this,0)` to load program 0.

**Every load path starts from a copy of T and then overlays the stored values.** So any index that a file or chunk does not
contain takes T's value:
* `.sy1` file: `FUN_18001e350` clears the slots, then `FUN_18001c700(dst, T)` copies all of T (including version 113). Next it
  sets `version=100` once the name line has been read. Then `ver=N` overrides it, and each `idx,value` line overrides/inserts.
* VST chunk / program (`FUN_180028cb0`, `FUN_18002a660`): `FUN_180024f60(local, T)` copies T. Then the entries 0..98 whose
  "present" flag is set are overlaid. The version is the chunk's version, or 100 if the chunk has no version flag.

### 1.3 Pipeline [O] (`FUN_180027c70`)
```
sd = copy(T); overlay file/chunk entries; sd.version = file ver (default 100)
FUN_180026a10(this, sd):                       // version conversion
  if sd.version < 113:
     tmp = empty
     for each (idx,v) in sd with idx<99 or idx>=250:
         v' = CONVERT(idx, v, sd.version)       // switch below
         if v' != v: tmp[idx] = v'
     sd |= tmp
  sd.version = 113
for each (idx,v) in sd (idx<99 || idx>=250):  apply via FUN_180028fe0(this, idx, v)
reset all 32 voices (FUN_18003cde0)
```
`FUN_180028fe0` clamps v to the descriptor's [min,max] (from `DAT_1801a0e80`, which `FUN_180037420` builds from the 0x180093f10
table). It skips the clamp when both are -1. It adds MIDI-controller modulation (see 2.8) and then dispatches per idx.

### 1.4 CONVERT(idx, v, ver) [O] (`FUN_180026a10`; the same chain for host automation is `FUN_1800278a0`)
This is a C `switch(ver)` with **fall-through**. The cases run in this order:

| ver entering | runs |
|---|---|
| 100 | C100 then C106 then C108 then C109 then C111 |
| 105, 106 | C106 then C108 then C109 then C111 |
| 107, 108 | C108 then C109 then C111 |
| 109 | C109 then C111 |
| 111 | C111 |
| 101..104, 110, 112 | **nothing** |

So the version sets are S100={100}, S106={100,105,106}, S108=S106 plus {107,108}, S109=S108 plus {109}, and S111=S109 plus {111}.

* **C100** (`FUN_180026c60`). `nearest(f, target)` means: return the n in 0..127 that minimises |f(n) - target|. The code scans
  n upward from 0 and stops when the error grows. Both curves are monotonic, so this is the argmin.
  * **C3** (idx 3, osc2 fine tune): `f=(x-0.5)*2`, snapped to -1, 0 or +1 when within 0.03. target = 2^(f/12).
    new(n) = 2^(c(n)/1200) with c(n) = n-62 for n<62, 0 for 62..66, n-66 for n>66 (cents; matches the engine case 3) [O; the pow args are I].
  * **CA** (idx 12, 15, 25 attacks): target = x<0.5 ? 1.75^(x^2) - 1 + 0.0005 : 30^(x^4.6) - 1. new(n) = 0.0005*56000^(n/127) (s).
  * **CS** (idx 27 amp sustain, 29 amp gain): target = x (old linear). new(n) = n==0 ? 0 : 2048^((n/127)^0.3)/2048.
  * **CV** (idx 30 amp vel sens): target = 1 - 0.9x. new(n) = 10^(-1.5*n/127).
  * **CL** (idx 43, 48 LFO speed): target = 20^x - 0.98333. new(n) = 1500^(n/127)/12 (Hz).
  * idx 67..72 := T[idx].
* **C106** (`FUN_180027150`, followed by inline overrides):
  * **CD** (idx 13, 16, 18, 26, 28 decays/releases): target = 60^(x^2.95) - 1. new(n) = 0.01*6000^(n/127) (s).
  * **CF** (idx 21 filter env amount): the old value was unipolar, target = x. new(n) = (n-63)/64 (bipolar), so n ≈ 63+64x.
  * idx 37 (delay dry/wet): v/2 (integer division).
  * **CCT** (idx 52 chorus delay time, ms): target = x<0.5 ? 0.3*50^(2x) : 30x. new(n) = y<0.5 ? 0.05*300^((2y)^0.6) : 30y, with y=n/127 [the 2y arg is I; continuity at 0.5 is checked].
  * **CCR** (idx 54 chorus rate): target = 10^(x^1.8) - 1 + 0.05. new(n) = n==0 ? 0.01 : 0.02*20000^(n/127).
  * **CCF** (idx 55 chorus feedback): target = 0.9*trunc(-v/127). That is 0 for v<127 and -0.9 for v=127 (this looks like an original bug; reproduce it).
    new(n) = (n-64)/64*0.99. Result: n=64, or n≈6 when v=127.
  * idx 75..85 and 90 := T[idx]. **Then** idx 84 := 64, idx 85 := 24, and idx 86..89 := 0.
* **C108** (`FUN_1800276f0`). P[k] is the value of index k in the patch being converted:
  * **CSENS** (idx 50 sens1 using P[44]; idx 51 sens2 using P[43]): `v' = 64 + trunc((1 - P/127) * (v/127) * 63)`.
    The old sens was unipolar and hard-routed to sy1 44/43 (lfo1 depth/speed, the same as T's assign defaults). The new sens is bipolar around 64.
  * **CUD** (idx 75 unison detune; P[38] is the play mode): `c = P[38]==0 ? 100 : 180`.
    `v' = min(127, trunc(63.5 * ln(1 + 0.01*(e^2-1)*(v*c/127 + 0.5))))`. This inverts the new exponential curve (2.5).
    Note: for S106 patches, idx 75 was already replaced by T=22 in C106, so these patches get CUD(22).
  * idx 86, 87, 88, 89, 91, 92, 94 := T[idx]. idx 93 := (P[38]==0 ? 2 : 4).
* **C109**: idx 95, 96, 97 := T[idx]. **C111**: idx 98 := T[idx].

Net effect for old files [O]. A v100 file (third-party, idx 0..66) gets all curve fixes, and 67..98 = T, except: 93 is 2 (poly)
or 4 (mono/legato), 75 = CUD(22), and 50/51 are rescaled. Factory ver=105 files get C106 onward. **In particular, idx 21 (filter
amount) in every factory patch is remapped from unipolar to bipolar.**

### 1.5 Default table (T, 0x180093af0) [O]
| sy1 | name | range | default T | conversion if ver<113 (applied in this order) |
|---|---|---|---|---|
| 0 | osc1 shape | 0..3 | 2 | - |
| 1 | osc2 shape | 1..4 | 1 | - |
| 2 | osc2 pitch | 0..127 | 64 | - |
| 3 | osc2 fine tune | 0..127 | 81 | S100: C3 |
| 4 | osc2 kbd track | 0..1 | 1 | - |
| 5 | osc mix | 0..127 | 64 | - |
| 6 | osc2 sync | 0..1 | 0 | - |
| 7 | osc2 ring modulation | 0..1 | 0 | - |
| 8 | osc pulse width | 0..127 | 64 | - |
| 9 | osc key shift | -24..24 | 0 | - |
| 10 | osc mod env on/off | 0..1 | 0 | - |
| 11 | osc mod env amount | 0..127 | 64 | - |
| 12 | osc mod env attack | 0..127 | 0 | S100: CA |
| 13 | osc mod env decay | 0..127 | 0 | S106: CD |
| 14 | filter type | 0..4 | 1 | - |
| 15 | filter attack | 0..127 | 0 | S100: CA |
| 16 | filter decay | 0..127 | 64 | S106: CD |
| 17 | filter sustain | 0..127 | 32 | - |
| 18 | filter release | 0..127 | 64 | S106: CD |
| 19 | *filter freq | 0..127 | 81 | - |
| 20 | *filter resonance | 0..127 | 14 | - |
| 21 | filter amount | 0..127 | 128 (! > max; clamped to 127 when applied) | S106: CF |
| 22 | filter kbd track | 0..127 | 64 | - |
| 23 | filter saturation | 0..127 | 0 | - |
| 24 | filter velocity switch | 0..1 | 1 | - |
| 25 | amp attack | 0..127 | 64 | S100: CA |
| 26 | amp decay | 0..127 | 64 | S106: CD |
| 27 | amp sustain | 0..127 | 107 | S100: CS |
| 28 | amp release | 0..127 | 64 | S106: CD |
| 29 | amp gain | 0..127 | 107 | S100: CS |
| 30 | amp velocity sens | 0..127 | 64 | S100: CV |
| 31 | arpeggiator type | 1..4 | 1 | - |
| 32 | arpeggiator oct range | 0..3 | 0 | - |
| 33 | arpeggiator beat | 0..18 | 11 | - |
| 34 | arpeggiator gate | 0..127 | 64 | - |
| 35 | delay time | 0..19 | 8 | - |
| 36 | delay feedback | 0..127 | 40 | - |
| 37 | delay dry/wet | 0..127 | 20 | S106: v/2 (int) |
| 38 | play mode type | 0..2 | 0 | - |
| 39 | portament time | 0..127 | 0 | - |
| 40 | pitch bend range | 0..24 | 12 | - |
| 41 | lfo1 destination | 1..7 | 2 | - |
| 42 | lfo1 type | 0..5 | 1 | - |
| 43 | lfo1 speed | 0..127 | 64 | S100: CL |
| 44 | lfo1 depth | 0..127 | 0 | - |
| 45 | osc1 FM | 0..127 | 0 | - |
| 46 | lfo2 destination | 1..7 | 5 | - |
| 47 | lfo2 type | 0..5 | 1 | - |
| 48 | lfo2 speed | 0..127 | 64 | S100: CL |
| 49 | lfo2 depth | 0..127 | 64 | - |
| 50 | midi ctrl sens1 | 0..127 | 74 | S108: CSENS(v, P[44]) |
| 51 | midi ctrl sens2 | 0..127 | 74 | S108: CSENS(v, P[43]) |
| 52 | chorus delay time | 0..127 | 64 | S106: CCT |
| 53 | chorus depth | 0..127 | 64 | - |
| 54 | chorus rate | 0..127 | 50 | S106: CCR |
| 55 | chorus feedback | 0..127 | 64 | S106: CCF |
| 56 | chorus level | 0..127 | 40 | - |
| 57 | lfo1 on/off | 0..1 | 1 | - |
| 58 | lfo2 on/off | 0..1 | 1 | - |
| 59 | arpeggiator on/off | 0..1 | 0 | - |
| 60 | equalizer tone | 0..127 | 64 | - |
| 61 | equalizer freq | 0..127 | 64 | - |
| 62 | equalizer level | 0..127 | 64 | - |
| 63 | equalizer Q | 0..127 | 64 | - |
| 64 | chorus type | 1..4 | 2 | - |
| 65 | delay on/off | 0..1 | 1 | - |
| 66 | chorus on/off | 0..1 | 1 | - |
| 67 | lfo1 tempo sync | 0..1 | 0 | S100: :=T |
| 68 | lfo1 key sync | 0..1 | 0 | S100: :=T |
| 69 | lfo2 tempo sync | 0..1 | 0 | S100: :=T |
| 70 | lfo2 key sync | 0..1 | 0 | S100: :=T |
| 71 | osc mod dest | 0..2 | 0 | S100: :=T |
| 72 | osc1,2 fine tune | 0..127 | 64 | S100: :=T |
| 73 | unison mode | 0..1 | 0 | - |
| 74 | portament auto mode | 0..1 | 0 | - |
| 75 | unison detune | 0..127 | 22 | S106: :=T; S108: CUD(v, P[38]) |
| 76 | osc1 detune | 0..127 | 0 | S106: :=T |
| 77 | effect on/off | 0..1 | 0 | S106: :=T |
| 78 | effect type | 0..9 | 0 | S106: :=T |
| 79 | effect control1 | 0..127 | 64 | S106: :=T |
| 80 | effect control2 | 0..127 | 64 | S106: :=T |
| 81 | effect level/mix | 0..127 | 64 | S106: :=T |
| 82 | delay type | 0..2 | 0 | S106: :=T |
| 83 | delay time spread | 0..127 | 66 | S106: :=T |
| 84 | unison pan spread | 0..127 | 64 | S106: :=T; S106: :=64 |
| 85 | unison pitch | 0..48 | 24 | S106: :=T; S106: :=24 |
| 86 | midi ctrl src1 | -1..-1 | 45057 (0xB001 = CC#1 mod wheel) | S106: :=0; S108: :=T |
| 87 | midi ctrl assign1 | -1..-1 | 44 (= sy1 44, lfo1 depth) | S106: :=0; S108: :=T |
| 88 | midi ctrl src2 | -1..-1 | 45057 (0xB001 = CC#1 mod wheel) | S106: :=0; S108: :=T |
| 89 | midi ctrl assign2 | -1..-1 | 43 (= sy1 43, lfo1 speed) | S106: :=0; S108: :=T |
| 90 | pan | 0..127 | 64 | S106: :=T |
| 91 | osc phase shift | 0..127 | 0 | S108: :=T |
| 92 | unison phase shift | 0..127 | 0 | S108: :=T |
| 93 | unison voice num | 2..8 | 2 | S108: P[38]==0 ? 2 : 4 |
| 94 | *(hidden, no VST name)* polyphony / voice count | 1..32 | 16 | S108: :=T |
| 95 | osc1 sub gain | 0..127 | 0 | S109: :=T |
| 96 | osc1 sub shape | 0..3 | 1 | S109: :=T |
| 97 | osc1 sub octave | 0..1 | 1 | S109: :=T |
| 98 | delay tone | 0..127 | 64 | S111: :=T |

The names and ranges come from the 0x180093f10 descriptor table. sy1 **94 has no descriptor**: the dispatcher sends it to
`FUN_1800370e0` (clamped to 1..total voices), and the float map gives it 1..32. So it is the **polyphony** setting [O].
Special indices ≥250 are also accepted in patches and chunks: 250 = version, 251/252 = bank/program (`FUN_1800288e0`),
254 = master volume [O for the paths, I for the meanings of 251/252].

### 1.6 Host automation: VST float ↔ int (`FUN_180025c10`, mode 0 = float→int, 1 = int→float) [O]
VST `setParameter` (Synth1VST `FUN_18001a3d0`): **VST index = sy1 index for 0..98**. It computes `i = F2I(idx, f, 113)` and then
`FUN_18002a4d0`. It caches f at +0xbc70 and i at +0xbdfc. idx 250 = "version": f*1000 selects the conversion version (0 means 100).
On change it reconverts all 99 cached floats with `F2I(...,ver)` followed by `FUN_1800278a0` (the CONVERT chain above). idx 254 = master volume.

| class (sy1 idx) | float→int | int→float |
|---|---|---|
| continuous 0..127: 2,3,5,8,11,12,13,15-23,25-30,34,36,37,39,43,44,45,48-56,60-63,72,75,76,79,80,81,83,84,90,91,92,95,98,252,254 | min(127, ⌊128f⌋) | i/128 + 1/256 |
| switches: 4,6,7,10,24,57,58,59,65-70,73,74,77,97 | f≥0.5 | 0.25 / 0.75 |
| 0, 96 (0..3) | quarters | .125/.375/.625/.875 |
| 1, 31 (1..4) | 1 + quarter index | .125+.25(i-1) |
| 9 key shift | min(48,⌊49f⌋)-24 | (i+24)/49+1/98 |
| 14 filter type | fifths | .1/.3/.5/.7/.9 |
| 32 arp oct | min(3,⌊4f⌋) (ver 100: +1) | i/4+1/8 (ver 100: (i-1)/4+1/8) |
| 33 arp beat | min(18,⌊19f⌋) | i/19+1/38 |
| 35 delay time | min(19,⌊20f⌋) | i/20+1/40 |
| 38 play mode | f<.5→0, f<.75→1, else 2 | 0→.125, 1→.625, 2→.875 |
| 40 bend range | min(24,⌊25f⌋) | i/25+1/50 |
| 41, 46 lfo dest | ver≤100: fifths→1..5. ver≤106: thresholds .25/.35/.45/.55/.65→1..6. else .25/.35/.45/.55/.65/.75→1..7 | the matching table |
| 42, 47 lfo type | thresholds .25,.35,.45,.55,.65 → **0,1,5,2,3,4** | 0→.2, 1→.3, 5→.4, 2→.5, 3→.6, 4→.7 |
| 64 chorus type | <.25→1, <.75→2, else 4 (3 cannot be reached; quirk) | 1→.125, 2→.375, else .875 |
| 71, 82 (0..2) | <.33→0, <.66→1, else 2 | .166/.5/.833 |
| 78 effect type | min(9,⌊10f⌋) | i/10+.05 |
| 85 unison pitch | min(48,⌊49f⌋) | i/49+1/98 |
| 86-89 ctrl src/assign | trunc(65536f)-1 | (i+1)/65536 |
| 93 unison voices | 2+min(6,⌊7f⌋) | (i-2)/7+1/14 |
| 94 polyphony | 1+min(31,⌊32f⌋) | (i-1)/32+1/64 |

`getParameterName` = `FUN_180019750` → `FUN_1800373a0` (looks up the name by sy1 idx). The GUI value text comes from
`FUN_180034950`, which is keyed by GUI control id (0x3ea..0x442), not by sy1 idx. It shows units, e.g. "%.2f msec" for chorus time,
"%.1f Hz/KHz" for EQ freq = 50*320^x, "%.1f db" for EQ level = (v-64)/127*50, "L/R %.0f%%" for pan, and "%s%.2d cent" for fine tunes [O].

---------------------------------------------------------------------------------------------------
## 2. Voice management

### 2.1 Objects [O]
* 32 `CVoice` objects at `this+0x38`, stride 0x4a8. Inside a voice: `+0x18` oscillator block, `+0x1e8` filter, `+0x3b8` amp envelope.
  Also `+0x480` enabled, `+0x484` porta-auto flag, `+0x488` unison pan offset, `+0x48c` unison phase-shift (v/127),
  `+0x490` unison N, `+0x494` index within the unison group, `+0x498` slot index, **`+0x49c` gate**, `+0x4a0` note, `+0x4a4` velocity.
* Two `CKeyboardInfo` objects: KB1 at `+0x9d70` (live MIDI) and KB2 at `+0xa388` (arp output).
  KB layout: `+0x08` held count, `+0x0c vel[128]` (0 = up), `+0x20c stack[]` (notes in the order pressed; a note gets +0x100 while it is "new, not yet allocated"), `+0x40c` sustain-pedal flag, `+0x410 sustained[128]`, `+0x610` dirty.
  noteOn `FUN_180028290`: an already-held note is first released, then it is pushed as note+0x100. noteOff `FUN_1800280c0` removes it from the stack.
* Voice allocator at `+0xa9a0` (ctor `FUN_180036270`): `+0x08 voice*[32]`, `+0x188` total=32, `+0x18c` play mode, `+0x190` polyphony (sy1 94),
  `+0x194` unison on, `+0x198` unison N, `+0x19c` detune, `+0x1a0` unison pitch (cents), `+0x1a4` pan spread, `+0x1a8` KB* (KB1, or KB2 when arp on; `FUN_1800370b0`),
  `+0x1b0` active voice count, `+0x1b4` last allocated slot, `+0x1b8` std::list<CVoice*> of allocated voices (oldest first), `+0x1c8` mono velocity.

### 2.2 Parameter → engine (from `FUN_180028fe0`) [O]
| sy1 | target |
|---|---|
| 38 play mode | `FUN_1800370c0`: 0 poly, 1 mono, 2 legato |
| 39 porta time | per-voice osc `FUN_18003e3d0(t)`, t = 2.5·x·e^{2.5(x-1)} s (0 → off; 127 → 2.5 s) |
| 40 bend range | `this+0x24` (short, semitones) |
| 73 unison mode | `FUN_180037150(v==1)`. Also sets `+0x9744` and flags in the delay/chorus objects 0x9a98/0x9b18 [U: effect] |
| 74 porta auto | voice `+0x484` |
| 75 unison detune | `FUN_180037130` |
| 84 unison pan spread | `FUN_180037250((v-64)/64)` |
| 85 unison pitch | `FUN_1800371a0`: cents = (v-24)·100 |
| 92 unison phase shift | voice `+0x48c = v/127` (`FUN_180052720`), passed to osc on trigger |
| 93 unison voice num | `FUN_1800371d0` |
| 94 polyphony | `FUN_1800370e0`, clamped 1..32 |
| 50/51 sens, 86-89 src/assign | `FUN_1800379b0` / `FUN_180037a10` / `FUN_1800378c0` (2.8) |
| 31,32,33,34,59 arp | 3 |

### 2.3 Active voices (`FUN_180036360`) [O]
`active = (mode==poly) ? polyphony : min(unison ? N : 1, polyphony)`. Voices at or above `active` get released and disabled.

### 2.4 Allocation (runs once per render sub-block when the KB is dirty: `FUN_180036eb0`)
**Poly** (`FUN_180036c00`) [O]:
```
for v in allocList: if KB.vel[v.note]==0 { v.release(); remove }          // key up -> release
G = unison ? N : 1
for each new note n at the top of the stack (newest first, at most active):
    release+remove every allocated voice playing n                         // same-note retrigger
    for k in 0..G-1:
        v = next voice with gate==0, scanning round-robin from lastSlot+1 over slots [0,active)
        if none: v = allocList.front() (OLDEST allocated); remove it        // voice steal
        lastSlot = v.slot
        v.noteOn(n, KB.vel[n]); v.group(N=G, k); v.trigger(); allocList.push_back(v)
clear the +0x100 "new" flags
```
Note that a voice in its release phase (gate 0) counts as free and is reused round-robin. Stealing only happens when every active voice is gated.

**Mono / Legato** (`FUN_180036930`) [O]: this mode uses the fixed voices 0..G-1. Priority is **last note** (top of the stack).
* No keys held → release all G voices.
* Top note new → store vel = KB.vel[n].
* Top note old (after a release returned to a still-held note) → if voice0 is gated and already on n, do nothing. Otherwise continue with the stored velocity.
* Mono: every voice runs noteOn(n, vel) + trigger, so envelopes always retrigger (including on return to a held note).
* Legato: every voice runs noteOn(n, vel), which changes the pitch. Trigger runs only when the voice is not gated.

`CVoice::noteOn` `FUN_1800526e0`: stores note/vel and calls osc `FUN_18003e0c0(note, glide)` with `glide = autoMode ? gate : 1`.
`trigger` `FUN_180052740`: gate=1, osc unison/phase setup `FUN_18003e5b0(N, k, phaseShift)`, amp env on (vel), filter env on (note), osc reset `FUN_18003db00`.
`release` `FUN_1800525f0`: gate=0, amp+filter env release.

### 2.5 Unison (`FUN_180036460`, inlined in `FUN_180037130`/`FUN_1800371a0`) [O]
```
D   = 100 * (e^(2*det/127) - 1) / (e^2 - 1)                 // cents, exponential (v1.09+); det=127 → 100 c
for slot s in 0..31:  i = s % N
   oscCents[s]    = D*i/(N-1) - D/2  + ((i odd) ? (unisonPitch-24)*100 : 0)   -> osc FUN_18003e020
   filterCents[s] = D*(i/N - 0.5)                                            -> filter FUN_18003b7d0 [I: key-track offset]
   pan[s]         = (i/(N-1))*2*sp - sp,  sp=(v84-64)/64                      -> voice+0x488
unison off or N==1 -> all offsets 0
```
The offsets belong to the voice slot (s % N), not to the order within the allocated group. Unison phase shift is applied in the oscillator from (N, k, v92/127) [U: exact formula, other agent].

### 2.6 Portamento (osc block = voice+0x18; `FUN_18003e3d0`, `FUN_18003e0c0`, render `FUN_18003db00`) [O]
* Total glide length `T = sr * t` samples (+0x194), so the **glide time is constant regardless of interval**.
* On noteOn with glide: `old=clamp(keyshift+prevNote,0,128)`, `new=clamp(keyshift+note,0,128)`.
  `offset0 = currentOffset + (old*128/12 - new*128/12)`. The pitch unit is 128/12 per semitone [I]. The glide restarts from the current pitch, so it stays continuous.
  `counter=0`. Without glide, the offset is 0.
* Render: `p = counter/T`, `offset = trunc(offset0 * e^{-4p} * (1-p))`. It is recomputed every `step` samples
  (step = max(32, T/|offset0|/10), adapted at 25%/55% thresholds). Pitch = base + offset.
* Auto mode: glide only when the voice is still gated (legato overlap in mono/legato). In poly mode, newly allocated voices are not gated, so they never glide.

### 2.7 Pitch bend [O]
`FUN_180028fb0(v14)`: `target = clamp(v14_signed/128, -63, 63)` (stored at +0xbb22). Each render sub-block (≤32 samples
while moving) `FUN_180028300` moves the current value (+0xbb20) one step toward the target. Then for every voice
`bendPitch = (cur*range*128/63)/12` (same pitch units) goes to the osc (+0x90). So ±63 maps to ±range semitones.

### 2.8 MIDI control (`FUN_180037660`, CSynthesizer1 secondary vtable [1]) [O]
* **MIDI learn / CC map**: for CC numbers below 120, `DAT_1801a0be0[cc]` gives an sy1 idx. The map is loaded from the "CCMap" setting and is empty by default.
  Value: `param = min + ccval % (max-min+1)`. This is modulo, not scaling. It sets the parameter permanently.
* **Two controller slots** (s=0,1 at this+0xbb28+16s: `{int src, float sens, int assign, float ctl}`):
  * src codes (13 entries, `DAT_180094e68`): 0xB001..0xB00B = CC1..CC11 ("#1:mod wheel"…"#11:expression"), 0xD000 channel aftertouch, 0xE000 pitch bend.
  * ctl: CC → val/127. AT → val/127. PB → ((msb-64)*128+lsb)/8192, which is bipolar.
  * sens(v) = (v≤63 ? v-63 : v-64)/63, giving ±1 with dead centre at 63/64.
  * When the src changes, ctl is reset to 0. When the assign changes, the old target is re-applied without modulation.
  * Effective value of the assigned param: `clamp(stored + (max-min+1)*ctl*sens, min, max)`, computed in `FUN_180028fe0`. Both slots add when they share a target.
  * Allowed assign targets: -1 or any descriptor whose "continuous" flag is set (list built in `FUN_180037420`).
  * Defaults (T): both slots use the mod wheel. Slot1 targets lfo1 depth (44), slot2 targets lfo1 speed (43), and both have sens 74 (= +0.159).

### 2.9 Note on/off routing [O]
MIDI noteOn `FUN_180028020` → KB1. When it is the first held key: arp reset (`FUN_1800020f0(vel)`) if the arp is on, and LFO1/2 key-sync retrigger (`FUN_1800152e0`) if enabled.
noteOff `FUN_180027f80` → KB1, or marks the key sustained when the pedal is down. When no key is left, the arp resets.
Arp output noteOn/noteOff (`FUN_1800280b0`/`FUN_180027ff0`) → KB2. When the arp is on, the allocator reads KB2 (sy1 59 → `FUN_1800370b0`).

---------------------------------------------------------------------------------------------------
## 3. CArpeggiator (at this+0x9d08; ctor `FUN_180001240`) [O]
Fields: `+0x10` sr, `+0x18` tempo (BPM, default 120), `+0x1c` step samples, `+0x20` oct range, `+0x24` gate = v/127,
`+0x28` ticks/step, `+0x2c` position, `+0x30` tie flag, `+0x38` KB1*, `+0x40` direction-down, `+0x44` current octave, `+0x48` current note,
`+0x4c` sounding note, `+0x50` velocity (the one captured at reset), `+0x54` type, `+0x58` on.

* **Step length**: `step = ticks * sr * 60 / (tempo * 24)` (24 ticks = quarter note). It is recomputed on beat, tempo or sr change, and the position is rescaled.
* **Beat table** `DAT_180093000` (ticks), idx 0..18:
  96 (1), 84 (2)+(4)+(8), 72 (2)+(4), 48 (2), 42 (4)+(8)+(16), 36 (4)+(8), 32 (1)/3, 24 (4), 21 (8)+(16)+(32), 18 (8)+(16), 16 (2)/3, 12 (8), 9 (16)+(32), 8 (4)/3, 6 (16), 4 (8)/3, 3 (32), 2 (16)/3, 1 (32)/3.
  The labels come from `PTR_DAT_180075810`. "(n)" means a 1/n note, "+" means dotted (tied) values, and "/3" means triplet.
* **Tempo sync**: processReplacing (`FUN_180019d00`) calls `getTimeInfo(kVstTempoValid)`. If tempo > 0 it calls `FUN_180028830(tempo)` → arp, delay, and the LFOs.
  The arp does not use host PPQ position, so the phase restarts at the first key press [O for the call, I for "no PPQ"].
* **Per step** (`FUN_180001b40`; render `FUN_1800285a0` cuts blocks at arp events):
  first noteOff the previous note. If gate==0, emit nothing. Otherwise pick the next note:
  * type 1 **up&down**: ascending through held pitches (sorted by pitch, not by press order), then up through octaves 0..range, then down. It turns at the ends.
  * type 2 **up**: ascending through pitches and octaves, then wraps to the lowest note at octave 0.
  * type 3 **down**: descending, starting from the top note at octave=range, then wraps.
  * type 4 **random**: random octave in 0..range, random held note (CRT `rand`).
  * output note = note + 12*octave (dropped if > 127), velocity = the captured velocity (+0x50).
  * note length = step*gate. If gate<1 the note is released after step*gate samples; with gate==1 it is held until the next step.
* oct range 0..3 = extra octaves. Reset (`FUN_1800020f0`): pos=0, octave=0, note=-1, down=(type==3).

---------------------------------------------------------------------------------------------------
## 4. Master volume and output gain [O]
* Master volume (special idx 254; `FUN_18002a8a0` / `FUN_18002a660`): `v` 0..127 (default 64).
  `gain = v==0 ? 0 : 10^(dB/20)`, with `dB = 20(x+0.01) - 1/(x+0.01) - 10`. Examples: v=64 → -1.67 dB (0.825), v=100 → +4.7 dB, v=127 → +9.2 dB.
* Applied to the final interleaved stereo buffer in `FUN_1800285a0`, after the effect chain at `+0x9668` (vtable+0x18 renders the voices and effects).
  If the gain is unchanged it is a plain multiply (`FUN_18001b5d0`). Otherwise `FUN_180028480` smooths it per float sample:
  `g += (target-g)*k`, `k = 1 - 0.5^(1/(0.002*sr))`. That is a 2 ms half-life per sample step; since L and R are interleaved, it advances twice per frame. It snaps when |Δ|<1.5e-5.
* Per-voice **amp gain** (sy1 29): amp `FUN_180037f40(g)`, with `g = x==0 ? 0 : 2048^(x^0.3)/2048` (2^{11(x^{0.3}-1)}). The default 107 gives 0.683.
  Amp velocity sens (sy1 30) is passed linearly (`FUN_180037f60(x)`). The C100 conversion suggests the minimum velocity gain is 10^{-1.5·sens} [I].
  Pan (sy1 90) is a synth-level object at this+0x9b98, not per voice: `(v-64)/64` for v≤64 and `(v-64)/63` above (`FUN_180005760`). The unison per-voice pan offset is separate (2.5).

## 5. Open items [U]
* The exact pitch unit / tuning table for osc pitch (128/12 per semitone is inferred from the porta and bend math).
* How the osc uses the unison phase shift, and the filter's use of the per-slot detune offset.
* The meanings of special idx 251/252, and the sustain-pedal CC64 path (it sets KB `+0x40c`; the handler is not traced).
