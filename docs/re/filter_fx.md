# Synth1 v1.13: filter (CVcf) and effects spec

Source: `Synth1 VST64.dll` (image base 0x180000000). Addresses are function entry points.
Tags: **[O]** observed in code, **[I]** inferred, **[U]** unknown.
Notation: `x = v/127` (v = 0..127 parameter value); `sr` = sample rate; `pow(a,b)` = a^b.
Helpers: 0x180004d80/0x180057cc0 = pow, 0x1800575e0/0x180057a50 = exp, 0x180056870/0x18005b840 = ln,
0x180056db0/0x18005a6c0 = sin, 0x180059360 = cos, 0x18005dca0 = tan, 0x18005aef8 = cosh [I from usage].

Parameter dispatcher: `CSynthesizer1::setParam(sy1idx, v)` **0x180028fe0** (switch on the sy1 index, applied to all 32 voices).

---------------------------------------------------------------------------------------------------
## 1. Master signal chain [O] (constructor 0x180025110; `FUN_18001b5c0(dev, src)` sets dev.input = src)

```
32 voices (each: osc -> CVcf -> amp)  -> CMixDevice (+0x9be0, voice sum, stereo)
 -> CEffector  (+0x9718)  effect block, types 0..9
 -> CEq        (+0x9a98)  parametric peak EQ (mode 0; init 1000 Hz, 0 dB, Q 8)
 -> CEq        (+0x9b18)  "tone" (1-pole LP or HP; init LP 10 kHz)
 -> CPan       (+0x9b98)  pan (LFO pan modulation hooks on the LFO objects at +0xab70 / +0xacd0)
 -> CDelay     (+0x9538)  tempo delay. Insert: CEq (+0x95e8) = delay tone, in the feedback loop
 -> CChorus    (+0x9668)  chorus/flanger. Receives the delay's wet bus separately (see section 7)
 -> output
```
The buffers are stereo-interleaved. Each device pulls from its `input` (vtable slot1 = mono, slot3 = stereo).

---------------------------------------------------------------------------------------------------
## 2. CVcf: voice filter (vtable 0x1800771a0, ctor 0x180037f70)

### 2.1 Parameter mapping (setParam cases) [O]
| sy1 | param | call | stored |
|---|---|---|---|
| 14 | filter type 0..4 | 0x18003b7b0 | `type` (+0xa4); forces a coefficient recompute |
| 19 | freq | 0x18003b6c0 | `base = int(x*1023)` (+0x90), index into the cutoff table |
| 20 | resonance | 0x18003b770 | `res = x` (+0x94) |
| 21 | env amount | 0x18003b670 | `amt = (v-63)/64` (+0xb4), range -0.984..+1.0 |
| 22 | key track | 0x18003b720 | `trk = x` (+0xa8) |
| 23 | saturation | 0x18003b6b0 | `sat = exp((x-1)*5.8) * x` (+0xc4) |
| 24 | velocity switch | 0x18003b870 | +0xbc |
| 15/16/18 | filter A/D/R | 0x18000d210/d260/d3a0 | A = 56000^x*0.0005, D and R = 6000^x*0.01 (units per the env agent's spec) |
| 17 | filter S | 0x18003b780 | x |

### 2.2 Cutoff table [O] (0x1800381f0, 1024 floats at 0x1801a1c10)
`F[i] = 15 * 1066.6667^(i/1024)`, i = 0..1023. This gives 15 Hz to about 15.99 kHz, 1024 steps over 10.06 octaves.
Knob mapping: `fc = F[int(v/127*1023)]`, so v=0 gives 15 Hz and v=127 gives about 16 kHz.

### 2.3 Key track [O] (0x180038070, recomputed at note-on 0x18003a780 and on knob change)
```
if trk > 0:
   f   = F[base] * pow(trk+1, (note-48)/12)          // note 48 = no shift; trk=1 -> 1 oct/oct
   idx = clamp(int(1024*ln(f/15)/ln(1066.6667)), 0, 1024)
   keyOfs (+0xac) = idx - base
else keyOfs = 0
```
The tracking curve is non-linear in trk. For example, trk=0.5 gives log2(1.5) = 0.585 oct/oct.

### 2.4 Envelope amount, velocity, headroom clamp [O] (0x180038140)
```
room = (1024 - base - keyOfs - centsOfs) / 1024
a    = velSwitch ? amt * velCurve/127 : amt
a    = clamp(a, room-1, room)       // env can never push the index past 0 or 1024
envDepth = a  -> envelope scaled to 1024*a index units (env full-scale 0x400 set in ctor, [I])
velCurve (0x18003a780) = int( (1-exp(-1.1*vel/127)) / (1-exp(-1.3)) * 127 )   // vel=127 -> 116
```
`centsOfs` (+0xb8) is set by 0x18003b7d0: `int(ln(2^(cents/1200)) * 1024/ln(1066.67))`, i.e. a cutoff offset in cents. Its caller is 0x180036460 (the source is probably a MIDI-controller or mod assignment) [U].

### 2.5 Per-block cutoff index [O] (process, slot7 0x18003ad50)
```
idx = base + keyOfs + centsOfs + env            // env = signed int from the filter ADSR
m   = mod1 + mod2                               // two mod-source inputs (LFO1/LFO2), 1/1024 units
idx += ((m > 0) ? (1024-idx) : idx) * m / 1024  // LFO scales into the remaining headroom
idx = clamp(idx, 0, 1023)
// glide: after a knob change (counter reset), idx moves linearly from the previous value
// to the target over sr/20 samples (50 ms). At note-on no glide occurs.
fc = F[idx]
dispatch: type 0,2,3 -> SVF 0x180038300 ; 1 -> LP24 0x1800390b0 ; 4 -> LPDL 0x1800398b0
```
**Type mapping [O]: 0 = LP12 (SVF low), 1 = LP24 (4-pole), 2 = HP12 (SVF high), 3 = BP12 (SVF band), 4 = LPDL (diode ladder).**

### 2.6 SVF, types 0/2/3 [O] (0x180038300)
Coefficients are recomputed when idx or res changes:
```
f   = 2*sin(pi*fc/(2*sr))        // double-sampled Chamberlin
r   = min(res, 1)
inG = 1 - 0.7*r
d   = r - 1                      // = -damping
per sample:
  x    = in*inG
  low += f*band ; band += f*((x-low) + d*band)            // pass 1
  low += f*band ; high = (x-low) + d*band ; band += f*high // pass 2
  out = LP:low | HP:high | BP:band
  if res > 0.9 and |out| > 20: out = ±20                   // hard clip only at high resonance
```
State is kept in float (+0xe4 low, +0xe8 high, +0xec band).

### 2.7 LP24, type 1 [O] (0x1800390b0). Stilson/Smith "Moog VCF" variant
```
f  = 2*fc/sr
k  = 3.6f - 1.6f^2 - 1 ;  p = (k+1)/2
p2 = (4.3f - 2.5f^2)/2 ;  r = res * exp((1-p2)*1.386249)    // separate polynomial for the res scale
k,p,r are smoothed every sample: c += (target-c)*(1 - 0.02^(1/(0.001*sr)))   (~1 ms)
per sample:
  x  = in - r*y4
  y1 = (x  + x_1 )*p - k*y1 ; y2 = (y1 + y1_1)*p - k*y2
  y3 = (y2 + y2_1)*p - k*y3 ; y4 = (y3 + y3_1)*p - k*y4     // *_1 = previous-sample value
  if res > 0.9: clip y4 to ±20 ;  out = y4                    // no cubic clipper
```
On the first run after a reset (`cached idx == -1`) the smoothed values jump straight to their targets.

### 2.8 LPDL, type 4 [O] (0x1800398b0; double precision). ZDF diode ladder
The structure matches D. Wurtz's KVR diode-ladder: a rational tanh plus a HPF in the feedback path [I].
```
OS  = (sr <= 48000) ? 2 : 1                      // "oversampled at 44.1/48k"
fc' = min(1.5*F[idx], 0.45*sr)
g   = tan(pi*fc'/(sr*OS));  G = g/(1+g);  a = G/2
k   = 15*res
fhp = fc'<100 ? 30 : fc'>=300 ? 120 : exp(ln30 + (ln fc'-ln100)/(ln300-ln100)*(ln120-ln30))
gh  = tan(pi*fhp/(sr*OS)); hpc = 2gh/(1+gh); kk = k/(1+gh); hfb = -1/(1+gh)
outGain = OS*0.65*(1 + 0.2*k)
c37=1/(1-a^2); c16=c37*a; c3=c16*a; c2=1/(1-c3); c4=c2*a; c17=G/(1-G*c4); c18=c3*c4
c19=1/(1-c4*a); c38=c37/(1+g); c37b=c37*a; c35=G*c19; c24=c2*a; c19b=c19/(1+g); c2b=c2/(1+g)
den = 1/(kk*c18*c17 + 1)
per input sample, repeat OS times (zero-stuffed: x = in on sub-step 0, x = 0 on later sub-steps):
  s3=z3/(1+g); s2=z2*c38+s3*c37b; s1=z1*c2b+s2*c24; s0=z0*c19b+s1*c35
  u = ((x - (s1*c3 + s0*c18 + s2*a + s3)*kk) - hp*hfb) * den
  u = |u|>3 ? sign(u) : u*(u*u+27)/(9*u*u+27)
  y0=u*(9/7)*c17+s0; y1=y0*c4+s1; y2=y1*c16+s2; y3=y2*a+s3
  z1=((y2+y0)/2 - z1)*G + y1 ; z2=((y3+y1)/2 - z2)*G + y2 ; z3=(y2/2 - z3)*G + y3
  z0=((y1 + u*9/7) - z0)*G + y0 ; hp += (y3*k - hp)*hpc
out = y2(last sub-step) * outGain        // no decimation filter
```
This transcription was checked against the decompiler only. Verify it numerically in the port.

### 2.9 Post-filter limiter and saturation [O] (slot9 0x18003a920, run on the whole voice block after the filter)
1. **Soft limiter** (0x18003af70): `e = max(|x|-1.1, 0); env = c*env + (1-c)*e` with `c = exp(-1/(0.005*sr))`. Output is `x * 1.1/(env+1.1)`. It is engaged only once some |x| > 1.1 and runs until env < 1e-5.
2. **Saturation** (applied only when sat != 0): `s100 = sat*100; y = x*(1+s100)/(1+s100*|x|)`. At v=127 the small-signal gain is 101 and the output saturates toward about ±1.01.

---------------------------------------------------------------------------------------------------
## 3. Effect block: CEffector (+0x9718, vtable 0x180073350, ctor 0x18000c140)

sy1 77 on/off, 78 type (0x18000c610), 79 ctl1 (0x18000c400), 80 ctl2 (0x18000c4e0), 81 level (0x18000c580). [O]
Every sub-effect receives every ctl update; the type selects which one runs.

| type | object | name [I] | ctl1 | ctl2 | level |
|---|---|---|---|---|---|
| 0 | CDistortion mode 0 | AD1 (3-stage "tube" drive) | drive | tone LP | out gain |
| 1 | CDistortion mode 1 | AD2 (soft clip) | drive | tone LP | out gain |
| 2 | CDistortion mode 2 | DIG (wave-fold) | drive | tone LP | out gain |
| 3 | CDecimator | LoFi | SR divide | bit depth | dry/wet |
| 4 | CRingMod | Ring | carrier freq | (none) | dry/wet |
| 5 | CCompressor | Comp | threshold+makeup | attack=release | output gain |
| 6..9 | CPhaser, stages 1,2,4,6 | Phaser | sweep range | rate | feedback |

### 3.1 CDistortion (vtable 0x180072fa8; mono 0x180008960, stereo 0x180008ac0) [O]
* drive `g = 10^((70x - 10)/20)`, i.e. -10..+60 dB (0x180008b40); level `L = (v+2)/129` (0x180008c70).
* Post tone, 2 RBJ biquads (0x180007630, designer 0x180008c90): LPF at `400*40^x2` Hz with Q=3.0, then HPF at 100 Hz with Q=2.3.
* **mode 0** (0x180007800, coefficients from 0x180007400 with all constants divided by sr):
```
kb1=540.354/sr kb2=829.380/sr kb3=1218.938/sr      // = 2π·86, 132, 194 Hz
kh =1/(194.779/sr+1)  (2π·31 Hz DC-block)   kl=41035.48/sr (2π·6531 Hz LP)
T(v) = v>=0 ? 1 - 1/(2v^2(v+0.4)+1) : 0          // one-sided "tube" curve
norm = 2 / T(g)
stage1: b1 += ((1-o1)*5 - b1)*kb1 ; o1 = T(g*x + b1) ; h1=(o1-o1_1+h1)*kh ; l1 += (h1-l1)*kl
stage2: b2 += ((1-o2) - b2)*kb2   ; o2 = T(l1 + b2)  ; h2=(o2-o2_1+h2)*kh ; l2 += (h2-l2)*kl
stage3: b3 += ((1-o3) - b3)*kb3   ; o3 = T(l2 + b3)  ; h3=(o3-o3_1+h3)*kh
y = L * norm * h3            // then the tone biquads
```
  (o_n here is the previous output of that stage. The bias tracks it to emulate a cathode/grid bias shift.)
* **mode 1** (0x180007a60): `u=(x+0.0001)*g; y = sign(u)*(1 - 1/((|u|+1)^2|u|+1)) * L*norm`, where `norm = 1/(1-1/((g+1)^2 g+1))`.
* **mode 2**: `u=g*x; t=0.7; if |u|>t: u = ||fmod(u-t, 4t)| - 2t| - t ; y = u*L*norm` (triangle fold).

### 3.2 CDecimator (vtable 0x180072df8, process 0x180005560) [O]
```
hold = v1 (0..127): sample is re-taken when (counter % (hold+1)) == 0
q    = int(x*65536) & (-1 << ((v2>>3)+1)) ; held = q/65536      // drops 1..16 LSBs of a 16-bit sample
y = mix*held + (1-mix)*x , mix = level/127
```

### 3.3 CRingMod (vtable 0x180073f48, process 0x180016d20) [O]
`fcar = 8000^x1` Hz (1..8000). Oscillator: magic circle, `w=2 sin(pi fcar/sr); s -= w*c; c += w*s`.
`y = x*(mix*s + 1 - mix)`, where mix = level/127.

### 3.4 CCompressor (vtable 0x180072d48, process 0x180004e40; contains 2 SimpleComp, L/R) [O]
```
Tdb = -20 - 40*x1 ; T = 10^(Tdb/20) ; makeup = 10^((-20-Tdb)/20)   (0..+40 dB)
tau_ms = 0.1 * 2000^sqrt(x2)   (0.1..200 ms, attack = release) ; c = exp(-1/(tau_ms*0.001*sr))
out  = 10^((30*x_level - 20)/20)  (-20..+10 dB)
per sample: e = max(|x|-T,0); env = c*env + (1-c)*e ; y = x*T/(T+env) * makeup*out
```
(The ratio setter is called with 0 and the kernel does not use it.)

### 3.5 CPhaser (vtable 0x180073ef8, kernel 0x1800167a0) [O]
```
fmin = 100*10^(1-x1) , fmax = 1000*10^x1        // ctl1 widens the sweep (100..1000 / 1000..10000 Hz)
rate = v2==0 ? 0.01 : 0.02*1000^x2  Hz          // 0.02..20 Hz
step = exp(ln(fmax/fmin) / (0.5*sr/rate))       // log-triangle sweep: f *= step, reverse at bounds
fb   = (level-64)/64*0.99 ; mix = 0.5 fixed ; N = stages (1,2,4,6)
per sample: f *= step ; v = x + fb*yprev ; a = (1 - 4f/sr)/(1 + 4f/sr)
  repeat N: t = s0 - v*a ; w = t*a ; s0 = w + v ; v = s1 - w ; s1 = v*a + t   // 2 allpass poles per stage
  yprev = v ; out = 0.5*x + 0.5*v
```

---------------------------------------------------------------------------------------------------
## 4. EQ and tone: CEq (vtable 0x180073438, coefficients 0x18000d4a0) [O]
* sy1 61 freq: `50*320^x` Hz (50..16000); 62 level: `(v-64)/127*50` dB (-25.2..+24.8); 63 Q: `10^x` (1..10).
* **Peak (mode 0)**: pole/zero placement with zero damping `dz` and pole damping `dp`. If G>=1: `dp=1/(2Q), dz=dp*G`. If G<1: `dz=1/(2Q), dp=dz/G`.
  `w=2πf/sr; r=exp(-w*d); θ = d<=1 ? cos(w*sqrt(1-d^2)) : cosh(w*sqrt(d^2-1))`.
  This gives `B=[1, -2 r_z θz, r_z^2]` and `A=[1, -2 r_p θp, r_p^2]`. B is then scaled so that |H(e^{jw})| = G at f.
  The block is bypassed when the level is 0 dB.
* **Tone (sy1 60)** (0x18002a5c0, second CEq): a 1-pole filter with bilinear transform and no prewarp.
  v<=64: LP, `fc = 200*110^(v/64)` (200 Hz..22 kHz; skipped if fc >= sr/2).
  v>64: HP, `fc = 10*800^((v-64)/64)` (10 Hz..8 kHz).
  `wc=2πfc, K=2sr: LP b0=b1=wc/(wc+K); HP b0=K/(wc+K), b1=-b0; a = (K-wc)/(wc+K)`; `y = b0 x + b1 x1 + a y1`.
* **Pan (sy1 90)** CPan 0x180016110: `p = (v-64)/64` (v<=64) or `(v-64)/63`. Gains are linear: `gL = p>0 ? 1-p : 1 ; gR = p<0 ? 1+p : 1`. There is no center boost. LFO modulation goes through 0x180015e10.

---------------------------------------------------------------------------------------------------
## 5. Delay: CDelay (+0x9538, vtable 0x180072e48, stereo process 0x180006810) [O]
* Buffer: 131072 stereo frames (mask 0x1ffff), i.e. 2.97 s at 44.1 kHz.
* sy1 35 time (0x180028ac0): `sec = ticks[t]*60/(bpm*24)` with 24 ticks per quarter note (table 0x180093a90):
  `ticks = [0,1,2,3,4,6,8,9,12,16,18,21,24,32,36,42,48,72,84,96]`, t = 0..19.
  In note values: 0, 1/96, 1/32T, 1/32, 1/16T, 1/16, 1/8T, 1/16., 1/8, 1/4T, 1/8., 1/8.., 1/4, 1/2T, 1/4., 1/4.., 1/2, 1/2., 1/2.., 1/1.
  Entry 0 = 0 is suspicious and may be offset in the UI [U].
  `D = floor4(sr*sec)`.
* sy1 83 spread (0x180025ae0): `s = sign(v-64) * 1000^(min(|v-64|/63,1)) * 1e-4` s (±0.1..100 ms; 0 at v=64).
  If s>=0 then `dL = D + s*sr`, otherwise `dR = D + |s|*sr`. Both are clamped to [sr*1e-4, buf-256] (0x1800059c0).
* sy1 36 feedback: `fb = min(x, 0.999)`. sy1 37 dry/wet: v<=64 gives dry=1, wet=(max(v,1)-1)/63. Otherwise wet=1, dry=(127-v)/63.
* Time change: the read positions glide, `d += (target-d)*(1-0.5^(1/(0.15 sr)))` (half-life 150 ms), with linear-interpolated reads.
* sy1 98 tone (0x180028b10), a CEq inside the feedback loop: v==64 is off. v<64: LP `200*110^(v/63)`. v>64: HP `10*800^((v-65)/62)`.
* Kernel 0x180006090 (types 0,1) / 0x180005b20 (type 2):
```
wetL = buf[w-dL].L ; wetR = buf[w-dR].R ; (wetL,wetR) = toneFilter(wetL,wetR)
type 0 (stereo) : buf[w].L = inL + fb*wetL ; buf[w].R = inR + fb*wetR
type 1 (cross)  : buf[w].L = inR + fb*wetR ; buf[w].R = inL + fb*wetL
type 2 (pingpong, two mono lines A,B; buffer sized ×2):
                  A[w] = (inL+inR)/2 + fb*wetR(from B) ; B[w] = wetL(from A)
output: dry bus = in*dry ; wet bus (separate buffer) = wet*(wetL,wetR)  -> summed at the chorus (section 7)
```
The type names (stereo/cross/ping-pong) are [I]. The behaviour is [O].

---------------------------------------------------------------------------------------------------
## 6. Chorus/flanger: CChorus (+0x9668, vtable 0x180072b18, stereo process 0x1800036d0) [O]
* Buffer: 8192 stereo frames. Write head = read head + D.
* sy1 52 time: x<0.5 gives `0.05*300^((2x)^0.6)` ms; x>=0.5 gives `30x` ms (0.05..30 ms). `D = sr*t`.
* sy1 53 depth: `dep = (100^x - 1)/99`. Modulation amplitude in samples is `M = D*dep*lim`, where `lim = min(1, (sr/rate)/(2π*D*dep))`. The limit keeps the slope at or below 1 sample per sample.
* sy1 54 rate: v=0 gives 0.01 Hz, otherwise `0.02*20000^x` Hz. The LFO is a magic-circle sine with `w=2 sin(pi rate/sr)`. LFO1 = s, LFO2 = c (90° apart).
* sy1 55 feedback: `fb = (v-64)/64*0.99`. sy1 56 level: `lvl = x`. sy1 64 type 1..4 (0x1800040e0): values outside 1..4 **and value 3** are forced to 2.
```
buf[w] = delayWet + lvl*in       // (or lvl*in if the delay has no wet output)
tap(ch, pos) = linear interpolation, 16.16 fixed point
type 1: m = M*s for both channels ; yL=tapL(r+m) ; yR=tapR(r+m)
type 2: mL = M*s, mR = M*c (quadrature)
type 4: p1=tapL(r+M s), p2=tapL(r-M s), q1=tapR(r+M c), q2=tapR(r-M c)
        yL = 0.9p1+0.1p2+0.7q1+0.3q2 ; yR = 0.1p1+0.9p2+0.3q1+0.7q2   (feedback uses p1,q1)
buf[r_write] += fb*y (per channel) ; out = in + y          // dry is always at unity
```
When the chorus is off: `out = in + delayWet`. When the chorus is on, the delay wet is heard **only through the chorus line**.

---------------------------------------------------------------------------------------------------
## 7. Gaps / unknowns
* [U] The UI names for effect types and delay types. The names above are guesses.
* [U] Delay table entry 0 = 0 ticks. Confirm against the UI display.
* [U] The source of the CVcf cents offset (+0xb8, set from 0x180036460).
* [U] The units and scale of the LFO mod inputs to the filter (`mod1+mod2` in 1/1024 headroom units). They come from 0x180014ce0 / 0x180014d10, which are in the LFO agent's scope.
* [I] LPDL equations: verify numerically. The decompiler's register naming may hide one swap.
* [U] The CDistortion stereo kernel 0x180008ac0 is assumed to be per-channel identical to the mono kernel.
* [U] The CEq peak normalisation uses cos/sin at w0. The exact cos/sin arguments were not fully traced.
