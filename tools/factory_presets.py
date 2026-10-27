"""Generate the ES 1 factory bank (presets/factory/NNN.sy1).

Every sound is written in musical units (Hz, seconds, semitones, octaves) and converted to the
raw 0..127 values of the .sy1 format with the engine's own curves (crates/es1-dsp). Parameter ids
and defaults are read from crates/es1-dsp/src/params.rs, so this script needs no other input.

    python tools/factory_presets.py            # rewrite presets/factory/
    python tools/factory_presets.py --zip X    # also write a zip bank (for es1-render --file)
"""

import math
import re
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "presets" / "factory"
VERSION = 113

# ---- parameter table -------------------------------------------------------------------------

_src = (ROOT / "crates/es1-dsp/src/params.rs").read_text(encoding="utf-8")
PARAMS = {}  # id -> (sy1, min, max)
for m in re.finditer(r'p!\((\d+), "(\w+)", "[^"]*", \w+, (-?\w+), (-?\w+), (-?\w+),', _src):
    sy1, pid, lo, hi = int(m[1]), m[2], int(m[3], 0), int(m[4], 0)
    PARAMS[pid] = (sy1, lo, hi)
assert len(PARAMS) == 99, len(PARAMS)

# ---- musical units -> raw values (inverse of the engine's curves) ----------------------------


def clamp(v, lo=0, hi=127):
    return max(lo, min(hi, int(round(v))))


def hz(f):
    """Filter cutoff: 15 * 1066.67^(v/127)."""
    return clamp(127 * math.log(f / 15) / math.log(1066.67))


def attack(s):
    """Attack time: 0.0005 * 56000^(v/127)."""
    return clamp(127 * math.log(max(s, 0.0005) / 0.0005) / math.log(56000))


def decay(s):
    """Decay / release time: 0.01 * 6000^(v/127)."""
    return clamp(127 * math.log(max(s, 0.01) / 0.01) / math.log(6000))


def level(a):
    """Amp sustain / gain: 2048^(x^0.3) / 2048."""
    if a <= 0:
        return 0
    return clamp(127 * (math.log2(a * 2048) / 11) ** (1 / 0.3))


def semis(n):
    """Osc 2 coarse pitch: floor(v * 120/127) - 60."""
    return clamp(math.ceil((n + 60) * 127 / 120))


def cents(c):
    """Osc 2 fine / master fine: 62..66 is a dead zone around 0."""
    return 64 if c == 0 else clamp(66 + c if c > 0 else 62 + c)


def octaves(o):
    """Filter envelope amount, bipolar around 63; 1024 index units = 10.06 octaves."""
    return clamp(63 + 64 * o / 10.06)


def lfo(f):
    """Free LFO rate: 1500^(v/127) / 12."""
    return clamp(127 * math.log(f * 12) / math.log(1500))


def bip(x):
    """Bipolar -1..1 around 64 (mod-env amount, spreads)."""
    return clamp(64 + x * 64 if x < 0 else 64 + x * 63)


def menv_semis(n):
    """Mod envelope amount for an osc 2 pitch sweep of n semitones (full scale = 60)."""
    return bip(n / 60)


# Choice raw values (see params.rs).
SINE, SAW, PULSE, TRI, NOISE = "sine", "saw", "pulse", "tri", "noise"
OSC1 = {SINE: 0, SAW: 1, PULSE: 2, TRI: 3}
OSC2 = {SAW: 1, PULSE: 2, TRI: 3, NOISE: 4}
SUB = {SINE: 0, TRI: 1, SAW: 2, "square": 3}
FILT = {"lp12": 0, "lp24": 1, "hp12": 2, "bp12": 3, "lpdl": 4}
LFO_WAVE = {SAW: 0, TRI: 1, "square": 2, "s&h": 3, "smooth": 4, SINE: 5}
LFO_DEST = {"osc2": 1, "pitch": 2, "filter": 3, "amp": 4, "pw": 5, "fm": 6, "pan": 7}
MENV_DEST = {"osc2": 0, "fm": 1, "pw": 2}
FX = {"drive1": 0, "drive2": 1, "fold": 2, "lofi": 3, "ring": 4, "comp": 5, "phase1": 6, "phase2": 7, "phase4": 8, "phase6": 9}
DELAY = {"stereo": 0, "cross": 1, "pingpong": 2}
DTIME = {"1/32": 3, "1/16T": 4, "1/16": 5, "1/8T": 6, "1/16.": 7, "1/8": 8, "1/4T": 9, "1/8.": 10, "1/4": 12, "1/4.": 14, "1/2": 16}
ARP_TYPE = {"updown": 1, "up": 2, "down": 3, "random": 4}
ARP_BEAT = {"1/4": 7, "1/8.": 9, "1/8": 11, "1/8T": 13, "1/16": 14, "1/16T": 15, "1/32": 16}
MODE = {"poly": 0, "mono": 1, "legato": 2}
WHEEL, PRESSURE = 0xB001, 0xD000

# ---- the blank sound every patch starts from --------------------------------------------------

BASE = dict(
    osc1_wave=OSC1[SAW], osc1_detune=0, osc1_fm=0,
    sub_level=0, sub_wave=SUB[SINE], sub_octave=1,
    osc2_wave=OSC2[SAW], osc2_sync=0, osc2_ring=0, osc2_track=1, osc2_pitch=semis(0), osc2_fine=cents(0),
    menv_on=0, menv_attack=0, menv_decay=decay(0.3), menv_amount=64, menv_dest=0,
    key_shift=0, osc_mix=0, pulse_width=127, osc_phase=0, fine_tune=64,
    flt_type=FILT["lp24"], flt_attack=0, flt_decay=decay(0.5), flt_sustain=64, flt_release=decay(0.3),
    flt_freq=hz(4000), flt_res=10, flt_amount=octaves(0), flt_sat=0, flt_track=64, flt_vel=0,
    amp_attack=attack(0.002), amp_decay=decay(1.0), amp_sustain=127, amp_release=decay(0.15), amp_gain=100, amp_vel=40,
    fx_on=0, fx_type=0, fx_ctl1=40, fx_ctl2=64, fx_level=100,
    eq_freq=64, eq_level=64, eq_q=64, eq_tone=64, pan=64,
    dly_on=0, dly_type=DELAY["stereo"], dly_time=DTIME["1/8."], dly_spread=64, dly_feedback=35, dly_tone=40, dly_mix=18,
    cho_on=0, cho_type=2, cho_time=64, cho_depth=64, cho_rate=50, cho_feedback=64, cho_level=40,
    play_mode=MODE["poly"], polyphony=16, porta_time=0, porta_auto=0,
    uni_on=0, uni_voices=2, uni_detune=22, uni_phase=0, uni_spread=64, uni_pitch=24, bend_range=2,
    # Mod wheel: vibrato (LFO 1 depth). Channel pressure: brighter (cutoff).
    ctl1_src=WHEEL, ctl1_dest=PARAMS["lfo1_depth"][0], ctl1_amount=67,
    ctl2_src=PRESSURE, ctl2_dest=PARAMS["flt_freq"][0], ctl2_amount=84,
    arp_on=0, arp_type=ARP_TYPE["up"], arp_range=0, arp_beat=ARP_BEAT["1/16"], arp_gate=80,
    lfo1_on=1, lfo1_wave=LFO_WAVE[TRI], lfo1_dest=LFO_DEST["pitch"], lfo1_speed=lfo(5.5), lfo1_depth=0, lfo1_sync=0, lfo1_key=0,
    lfo2_on=1, lfo2_wave=LFO_WAVE[SINE], lfo2_dest=LFO_DEST["filter"], lfo2_speed=lfo(0.3), lfo2_depth=0, lfo2_sync=0, lfo2_key=0,
)
assert set(BASE) == set(PARAMS), set(PARAMS) ^ set(BASE)

# Shorthands for recurring blocks.


def amp(a=0.002, d=1.0, s=1.0, r=0.15):
    return dict(amp_attack=attack(a), amp_decay=decay(d), amp_sustain=level(s), amp_release=decay(r))


def fenv(o, a=0.0, d=0.5, s=0.0, r=0.3):
    return dict(flt_amount=octaves(o), flt_attack=attack(a) if a else 0, flt_decay=decay(d), flt_sustain=clamp(s * 127), flt_release=decay(r))


def delay(time="1/8.", fb=35, mix=18, kind="stereo", tone=40, spread=64):
    return dict(dly_on=1, dly_time=DTIME[time], dly_feedback=fb, dly_mix=mix, dly_type=DELAY[kind], dly_tone=tone, dly_spread=spread)


def chorus(kind=2, level=40, rate=50, depth=64, time=64):
    return dict(cho_on=1, cho_type=kind, cho_level=level, cho_rate=rate, cho_depth=depth, cho_time=time)


def unison(n, detune=22, spread=0.6):
    return dict(uni_on=1, uni_voices=n, uni_detune=detune, uni_spread=bip(spread), polyphony=32)


def fx(kind, c1=40, c2=64, lvl=100):
    return dict(fx_on=1, fx_type=FX[kind], fx_ctl1=c1, fx_ctl2=c2, fx_level=lvl)


def menv(dest, amount, d, a=0.0):
    return dict(menv_on=1, menv_dest=MENV_DEST[dest], menv_amount=amount, menv_decay=decay(d), menv_attack=attack(a) if a else 0)


def lfo1(dest, f, depth, wave=TRI, key=0):
    return dict(lfo1_dest=LFO_DEST[dest], lfo1_speed=lfo(f), lfo1_depth=depth, lfo1_wave=LFO_WAVE[wave], lfo1_key=key)


def lfo2(dest, f, depth, wave=SINE, key=0):
    return dict(lfo2_dest=LFO_DEST[dest], lfo2_speed=lfo(f), lfo2_depth=depth, lfo2_wave=LFO_WAVE[wave], lfo2_key=key)


def arp(kind="up", rng=1, beat="1/16", gate=60):
    return dict(arp_on=1, arp_type=ARP_TYPE[kind], arp_range=rng - 1, arp_beat=ARP_BEAT[beat], arp_gate=gate)


def mono(glide=0.0, legato=True, auto=True):
    d = dict(play_mode=MODE["legato" if legato else "mono"])
    if glide:
        # porta = 2.5 x e^(2.5 (x-1)); invert numerically.
        x = min((i / 127 for i in range(1, 128)), key=lambda x: abs(2.5 * x * math.exp(2.5 * (x - 1)) - glide))
        d.update(porta_time=clamp(x * 127), porta_auto=1 if auto else 0)
    return d


def o1(w):
    return dict(osc1_wave=OSC1[w])


def o2(w, st=0, ct=0, mix=None):
    d = dict(osc2_wave=OSC2[w], osc2_pitch=semis(st), osc2_fine=cents(ct))
    if mix is not None:
        d["osc_mix"] = mix
    return d


def filt(kind, f, res=10, track=64, sat=0, vel=0):
    return dict(flt_type=FILT[kind], flt_freq=hz(f), flt_res=res, flt_track=track, flt_sat=sat, flt_vel=vel)


def sub(wave, lvl, down=True):
    return dict(sub_wave=SUB[wave], sub_level=lvl, sub_octave=1 if down else 0)


# ---- the bank ----------------------------------------------------------------------------------
# 64 sounds, grouped: bass, lead, pad, keys, brass & strings, pluck & sequence, effects.
# Names are tongue-in-cheek homages to West German household electronics of the 1970s. They are
# written in Latin-1, the format's encoding, so umlauts survive.

BANK = []


def patch(name, *parts, **kw):
    assert len(name) <= 22, name
    d = dict(BASE)
    for p in parts:
        d.update(p)
    d.update(kw)
    BANK.append((name, d))


# Bass
patch("Filterkaffee FK 1", o1(SAW), o2(SAW, -12, mix=40), sub(SINE, 30), filt("lp24", 160, res=72, sat=30, vel=1),
      fenv(4.0, d=0.35, s=0.1, r=0.15), amp(d=1.2, s=0.9, r=0.08), mono(), amp_gain=104)
patch("Trockenrasur Bass", o1(PULSE), o2(SAW, 0, 7, mix=50), filt("lpdl", 300, res=40, sat=20, vel=1),
      fenv(3.0, d=0.25, s=0.3), amp(d=1.0, s=0.9, r=0.08), mono(), fx("drive2", c1=40, c2=70, lvl=86), pulse_width=40)
patch("Rechenwerk Sub", o1(SINE), o2(TRI, 12, mix=18), sub(SINE, 20), filt("lp24", 2000, res=0, sat=25),
      amp(d=0.5, s=0.8, r=0.08), mono(), amp_gain=108)
patch("Entsafter Acid", o1(SAW), filt("lpdl", 220, res=106, sat=60, vel=1), fenv(5.0, d=0.2, s=0.0, r=0.1),
      amp(d=0.6, s=0.85, r=0.06), mono(glide=0.06), delay("1/8.", fb=30, mix=14))
patch("Toaster Krümel", o1(SAW), o2(PULSE, -12, mix=45), filt("lp24", 600, res=30, sat=40), fenv(2.0, d=0.3, s=0.4),
      amp(d=1.0, s=0.9, r=0.1), mono(), fx("drive1", c1=62, c2=55, lvl=58))
patch("Tiefkühltruhe Sub", o1(SINE), sub(SINE, 18), filt("lp24", 3000, res=0), amp(d=0.8, s=0.9, r=0.1), mono(),
      amp_gain=112)
patch("Sparschäler Pluck", o1(PULSE), o2(SAW, 12, mix=40), filt("lp24", 260, res=35, vel=1), fenv(4.0, d=0.14),
      amp(d=0.5, s=0.4, r=0.1), mono(legato=False), pulse_width=110)
patch("Funktionsbass FM", o1(SINE), o2(TRI, 12, mix=0), filt("lp24", 3000, res=0), menv("fm", bip(0.55), 0.25),
      amp(d=0.9, s=0.7, r=0.08), mono(), osc1_fm=34, amp_gain=106)
patch("Kompaktanlage Bass", o1(SAW), sub("square", 30), filt("lp24", 400, res=20, vel=1), fenv(3.0, d=0.4, s=0.3),
      unison(3, detune=30, spread=0.3), amp(d=1.0, s=0.9, r=0.1), mono())
patch("Zentralheizung Bass", o1(TRI), o2(SAW, 0, 5, mix=30), filt("lp12", 600, res=10, sat=40), fenv(1.0, d=0.6, s=0.5),
      amp(d=1.0, s=0.9, r=0.12), amp_gain=104)

# Lead
patch("Weniger aber Lead", o1(PULSE), o2(SAW, 12, mix=25), filt("lp24", 2500, res=25), fenv(1.5, d=0.4, s=0.6),
      amp(d=1.0, s=1.0, r=0.2), mono(glide=0.06), delay("1/8.", fb=35, mix=18))
patch("Taschenradio T1", o1(SAW), o2(PULSE, 0, 5, mix=45), filt("lp12", 2500, res=35, track=127), amp(r=0.2), mono(),
      fx("lofi", c1=80, c2=70, lvl=50), eq_tone=100, eq_freq=80, eq_level=84, eq_q=50, amp_gain=108)
patch("Zündung Sync", o2(SAW, 12, mix=127), filt("lp24", 4000, res=10), menv("osc2", menv_semis(24), 0.6),
      amp(r=0.2), mono(glide=0.04), fx("drive1", c1=30, c2=80, lvl=92), osc2_sync=1)
patch("Föhnfrisur Lead", o1(SAW), o2(SAW, 12, mix=30), filt("lp24", 3000, res=30, sat=50), unison(3, detune=25),
      amp(r=0.25), mono(), fx("drive2", c1=35, c2=80, lvl=80), delay("1/4", fb=30, mix=15, kind="pingpong"))
patch("Rasierwasser Sinus", o1(TRI), o2(TRI, 12, mix=15), filt("lp12", 2000, res=0), lfo1("pitch", 5.5, 3, key=1),
      amp(a=0.02, r=0.3), mono(glide=0.12), delay("1/4", fb=40, mix=22), chorus(level=25), amp_gain=106)
patch("Gute Form Supersaw", o1(SAW), o2(SAW, 12, mix=30), filt("lp24", 5000, res=10), unison(7, detune=40, spread=0.8),
      amp(a=0.005, r=0.3), chorus(level=30), delay("1/8", fb=30, mix=16, kind="pingpong"), osc1_detune=40, amp_gain=88)
patch("Elektropfeife", o1(SINE), filt("lp24", 8000, res=0), lfo1("pitch", 5.8, 3, key=1), amp(a=0.03, r=0.2),
      mono(glide=0.05), delay("1/4.", fb=40, mix=20), key_shift=12, amp_gain=104)
patch("Diaprojektor Solo", o1(PULSE), o2(PULSE, 12, 4, mix=30), filt("lp24", 2000, res=30), fenv(2.0, d=0.5, s=0.5),
      lfo2("pw", 0.8, 40, wave=TRI), amp(r=0.2), mono(glide=0.05), pulse_width=80)
patch("Bildschirmtext Lead", o1(PULSE), filt("lp24", 12000, res=0), lfo1("pitch", 6.0, 2, key=1), amp(r=0.1), mono(),
      fx("lofi", c1=90, c2=40, lvl=60), pulse_width=64)
patch("Wählscheibe FM", o1(SINE), o2(TRI, 12, mix=0), filt("lp24", 3000, res=10), menv("fm", bip(0.4), 0.3),
      amp(r=0.2), mono(glide=0.05), fx("drive1", c1=30, c2=70, lvl=90), osc1_fm=30, amp_gain=104)

# Pad
patch("Schneewittchensarg", o1(SAW), o2(TRI, 24, mix=35), filt("lp12", 3500, res=20), fenv(1.0, a=1.0, d=2.0, s=0.6),
      amp(a=0.8, d=2.0, s=0.8, r=2.5), chorus(kind=3, level=60), delay("1/4", fb=45, mix=22, kind="pingpong"),
      osc1_detune=50, amp_gain=96)
patch("Sonntagsfahrverbot", o1(SAW), o2(SAW, -12, 8, mix=50), filt("lp24", 350, res=35), fenv(2.5, a=3.0, d=4.0, s=0.5, r=4.0),
      lfo2("filter", 0.12, 30, wave=TRI), amp(a=2.5, d=3.0, s=0.9, r=4.0), chorus(level=45),
      delay("1/2", fb=50, mix=25), osc1_detune=60, amp_gain=104)
patch("Wohnlandschaft", o1(SAW), o2(SAW, 0, 7, mix=45), filt("lp12", 1200, res=15), unison(4, detune=30, spread=0.6),
      amp(a=0.4, d=2.0, s=0.9, r=1.5), chorus(level=50), amp_gain=90)
patch("Orange Hängelampe", o1(PULSE), o2(PULSE, 0, 6, mix=50), filt("lp24", 1600, res=10), lfo1("pw", 0.6, 35),
      amp(a=0.5, d=2.0, s=0.85, r=1.2), chorus(kind=1, level=50), pulse_width=96, amp_gain=104)
patch("Teakholz Streicher", o1(SAW), o2(SAW, 0, 9, mix=50), filt("lp12", 3000, res=5), fenv(0.7, a=0.4, d=1.0, s=0.8),
      amp(a=0.35, d=1.0, s=0.9, r=1.0), chorus(kind=3, level=60), osc1_detune=40, eq_tone=70, amp_gain=98)
patch("Cordsofa Wärme", o1(TRI), o2(TRI, 12, 6, mix=30), filt("lp24", 1000, res=0), amp(a=0.6, d=2.0, s=0.9, r=1.5),
      chorus(level=40), delay("1/4", fb=35, mix=15), amp_gain=110)
patch("Raumteiler Chor", o1(SAW), o2(SAW, 0, 10, mix=50), filt("bp12", 900, res=40), lfo2("filter", 0.3, 12),
      amp(a=0.5, d=2.0, s=0.9, r=1.2), chorus(kind=4, level=60), eq_freq=88, eq_level=80, eq_q=40, amp_gain=104)
patch("Mondbasis Alpha", o1(SAW), o2(PULSE, 7, mix=40), filt("lp24", 700, res=60), lfo2("filter", 0.08, 40, wave=TRI),
      amp(a=1.5, d=3.0, s=0.9, r=3.0), delay("1/4", fb=55, mix=30, kind="pingpong"), fx("phase2", c1=60, c2=20, lvl=60))
patch("Kristallvase", o1(SINE), o2(TRI, 19, mix=0), filt("lp12", 6000, res=0), amp(a=0.02, d=3.0, s=0.4, r=3.0),
      chorus(level=40), delay("1/4.", fb=50, mix=25), osc1_fm=28, amp_gain=104)
patch("Ferienprospekt", o1(SAW), o2(SAW, 0, 5, mix=50), filt("lp12", 4000, res=10), unison(3, detune=22),
      amp(a=0.25, d=2.0, s=0.9, r=1.2), fx("phase4", c1=50, c2=25, lvl=60), amp_gain=92)

# Keys
patch("Elektroklavier E70", o1(SINE), o2(TRI, 0, mix=0), filt("lp12", 5000, res=0), menv("fm", bip(0.5), 0.8),
      amp(d=2.5, s=0.15, r=0.4), chorus(kind=1, level=35), osc1_fm=40, amp_vel=90, amp_gain=108)
patch("Fondueparty Piano", o1(TRI), o2(TRI, 12, mix=0), filt("lp12", 5000, res=0), menv("fm", bip(0.4), 0.4),
      amp(d=2.0, s=0.2, r=0.4), chorus(kind=3, level=55, rate=60), osc1_fm=34, amp_vel=80, amp_gain=104)
patch("Rechenschieber Clav", o1(PULSE), o2(PULSE, 12, mix=20), filt("lp24", 900, res=35, vel=1), fenv(3.0, d=0.2, s=0.25),
      amp(d=0.7, s=0.35, r=0.1), fx("drive2", c1=45, c2=85, lvl=110), pulse_width=70, amp_vel=80)
patch("Taschenrechner Bell", o1(SINE), o2(TRI, 19, mix=0), filt("lp12", 8000, res=0), menv("fm", bip(0.3), 1.2),
      amp(d=3.0, s=0.0, r=2.5), delay("1/8.", fb=40, mix=20, kind="pingpong"), osc1_fm=45, amp_gain=108)
patch("Wohnzimmerorgel", o1(SINE), o2(TRI, 12, mix=40), sub(SINE, 40), filt("lp24", 6000, res=0),
      amp(d=0.1, s=1.0, r=0.06), chorus(kind=4, level=60, rate=90, depth=50), fx("drive1", c1=25, c2=90, lvl=96))
patch("Kirchentag Orgel", o1(PULSE), o2(SAW, 12, mix=30), sub("square", 25), filt("lp12", 2500, res=0),
      amp(a=0.03, d=1.0, s=1.0, r=0.3), delay("1/4", fb=30, mix=20), amp_gain=94)
patch("Studio Vibraphon", o1(SINE), o2(TRI, 24, mix=0), filt("lp12", 8000, res=0), menv("fm", bip(0.3), 0.3),
      lfo1("amp", 5.5, 15), amp(d=2.5, s=0.0, r=1.2), chorus(level=30), osc1_fm=20, amp_gain=110)
patch("Spieluhr Mechanik", o1(TRI), o2(TRI, 24, mix=30), filt("lp12", 8000, res=0), amp(d=1.2, s=0.0, r=1.0),
      delay("1/8.", fb=40, mix=25, kind="pingpong"), key_shift=12, amp_gain=108)

# Brass & strings
patch("Tagesschau Fanfare", o1(SAW), o2(SAW, 0, 6, mix=50), filt("lp24", 500, res=15, sat=20, vel=1),
      fenv(3.5, a=0.05, d=0.5, s=0.45, r=0.2), amp(a=0.03, d=1.0, s=0.8, r=0.25), chorus(level=30), amp_gain=96)
patch("Bundesbahn Bläser", o1(SAW), o2(SAW, 0, 4, mix=45), filt("lp24", 400, res=10), fenv(3.0, a=0.3, d=1.2, s=0.6),
      unison(3, detune=18), amp(a=0.15, d=1.0, s=0.9, r=0.4), lfo1("pitch", 5.0, 1), amp_gain=92)
patch("Schlagerstreicher", o1(SAW), o2(SAW, 12, 8, mix=35), filt("lp12", 4000, res=0), amp(a=0.3, d=1.0, s=0.9, r=0.9),
      chorus(kind=3, level=70, rate=40, depth=80), lfo1("pitch", 5.5, 2), osc1_detune=60, eq_tone=75, amp_gain=100)
patch("Hitparade Synthbrass", o1(SAW), o2(PULSE, 0, 5, mix=40), filt("lp24", 700, res=25, vel=1), fenv(3.0, d=0.3, s=0.5),
      unison(2, detune=25), amp(a=0.01, d=1.0, s=0.9, r=0.2), fx("drive1", c1=25, c2=75, lvl=96), amp_gain=94)
patch("Intercity Streicher", o1(SAW), o2(SAW, -12, 4, mix=40), filt("lp24", 1300, res=0), amp(a=0.6, d=1.0, s=0.9, r=1.5),
      chorus(level=60), delay("1/4", fb=40, mix=18), osc1_detune=45, amp_gain=100)
patch("Fernsehballett", o1(SAW), o2(SAW, 0, 8, mix=50), filt("lp12", 600, res=10), fenv(3.0, d=0.15),
      amp(d=0.3, s=0.0, r=0.25), chorus(kind=3, level=50), delay("1/8", fb=25, mix=15), amp_gain=108)

# Pluck & sequence
patch("Lochkarte Arp", o1(PULSE), filt("lp24", 550, res=45), fenv(3.5, d=0.15), amp(d=0.4, s=0.0, r=0.1),
      arp("up", 2, "1/16", 70), delay("1/8.", fb=40, mix=22, kind="pingpong"), pulse_width=110, amp_gain=108)
patch("Rechenzentrum", o1(TRI), o2(PULSE, 12, mix=20), filt("lp12", 3000, res=0), amp(d=0.12, s=0.0, r=0.08),
      arp("random", 3, "1/16", 50), delay("1/16", fb=45, mix=25, kind="pingpong"), amp_gain=110)
patch("Fernschreiber", o1(PULSE), filt("hp12", 250, res=20), amp(d=0.15, s=0.0, r=0.05), arp("updown", 1, "1/16T", 60),
      fx("drive2", c1=55, c2=90, lvl=100), pulse_width=80, amp_gain=110)
patch("Mondlandung Arp", o1(SAW), o2(SAW, 12, 6, mix=30), filt("lp24", 700, res=50), fenv(3.0, d=0.2),
      lfo2("filter", 0.1, 25, wave=TRI), amp(d=0.4, s=0.0, r=0.2), arp("up", 3, "1/8T", 60),
      delay("1/4.", fb=55, mix=30, kind="pingpong"))
patch("Zitruspresse Pluck", o1(SAW), o2(SAW, 12, 5, mix=40), filt("lp24", 420, res=30), fenv(5.0, d=0.18),
      amp(d=0.5, s=0.0, r=0.3), chorus(level=35), delay("1/8.", fb=35, mix=18), amp_gain=104)
patch("Kugelkopf Pluck", o1(SINE), o2(TRI, 12, mix=0), filt("lp12", 8000, res=0), menv("fm", bip(0.6), 0.08),
      amp(d=0.35, s=0.0, r=0.2), delay("1/16", fb=30, mix=15, kind="pingpong"), osc1_fm=20, amp_gain=110)
patch("Stoppuhr Sequenz", o1(SAW), filt("lpdl", 400, res=65, sat=40), fenv(4.0, d=0.12), amp(d=0.35, s=0.0, r=0.08),
      arp("down", 2, "1/16", 60))
patch("Datenträger Klick", o2(TRI, 0, mix=127), filt("lp24", 8000, res=0), menv("osc2", menv_semis(36), 0.05),
      amp(d=0.15, s=0.0, r=0.1), delay("1/8", fb=30, mix=15, kind="pingpong"), amp_gain=112)
patch("Hollerith Arp", o1(PULSE), filt("lp24", 1200, res=20), fenv(1.5, d=0.25, s=0.2), lfo1("pw", 0.5, 30),
      amp(d=0.6, s=0.5, r=0.2), arp("updown", 2, "1/8", 90), chorus(level=35), delay("1/4", fb=40, mix=20),
      pulse_width=96, amp_gain=104)
patch("Kassettenrekorder", o1(TRI), o2(SAW, 0, mix=30), filt("lp12", 1500, res=0), lfo1("pitch", 0.9, 2, wave=SINE),
      amp(d=1.0, s=0.3, r=0.4), fx("lofi", c1=95, c2=80, lvl=50), amp_gain=108)

# Effects
patch("Staubsauger", o2(NOISE, mix=127), filt("bp12", 600, res=50), fenv(3.0, a=1.5, d=1.0, s=1.0, r=1.5),
      lfo2("filter", 0.2, 20, wave=TRI), amp(a=0.8, s=1.0, r=1.5), mono(legato=False), amp_gain=100)
patch("Haartrockner Wind", o2(NOISE, mix=127), filt("lp12", 2200, res=30), lfo1("filter", 0.4, 30, wave="smooth"),
      lfo2("pan", 0.2, 40), amp(a=1.0, s=1.0, r=2.0), amp_gain=100)
patch("Kurzwelle Störung", o1(SAW), o2(PULSE, 7, mix=70), filt("lp24", 2500, res=30), lfo1("osc2", 8.0, 30, wave="s&h"),
      amp(r=0.3), delay("1/8", fb=45, mix=25, kind="pingpong"), osc2_ring=1, amp_gain=100)
patch("Weltempfänger", o1(SINE), o2(SAW, 0, mix=60), filt("hp12", 150, res=10), lfo2("osc2", 0.15, 40, wave=SAW),
      amp(a=0.2, r=0.8), fx("lofi", c1=85, c2=70, lvl=100), delay("1/4", fb=40, mix=20), osc2_ring=1, amp_gain=104)
patch("Elektrorasierer", o1(PULSE), filt("bp12", 1200, res=30), lfo1("amp", 40, 40, wave="square"), amp(r=0.1), mono(),
      fx("drive2", c1=60, c2=70, lvl=70), pulse_width=24, key_shift=-12)
patch("Kaffeemühle", o1(SAW), o2(NOISE, mix=60), filt("lp24", 500, res=50), lfo1("filter", 15, 40, wave="s&h"),
      amp(a=0.05, r=0.3), mono(), fx("fold", c1=50, c2=60, lvl=70))
patch("Bildröhre Brummen", o1(SAW), o2(PULSE, 0, -9, mix=50), filt("lp24", 400, res=55), lfo2("filter", 0.2, 15),
      amp(a=0.3, s=1.0, r=1.0), fx("drive1", c1=50, c2=50, lvl=100), key_shift=-12, amp_gain=104)
patch("Wecker Rrring", o1(PULSE), o2(PULSE, 17, mix=50), filt("hp12", 700, res=20), lfo1("amp", 14, 127, wave="square"),
      amp(r=0.1), key_shift=12, osc2_ring=1, amp_gain=104)
patch("Raumkapsel", o1(SINE), filt("lp24", 8000, res=0), lfo1("pitch", 7.0, 35, wave="s&h"), amp(r=0.3),
      delay("1/8", fb=55, mix=30, kind="pingpong"), amp_gain=106)
patch("Rohrpost", o2(NOISE, mix=127), filt("hp12", 200, res=40), fenv(4.0, a=0.4, d=0.6), amp(a=0.3, d=0.7, s=0.0, r=0.5),
      delay("1/4", fb=40, mix=25, kind="pingpong"), amp_gain=100)

# Level trims in dB, fitted by rendering the bank so every sound's loudest 300 ms window sits
# near the same RMS. Drives and folders are trimmed after the nonlinearity (effect level),
# everything else at the amp.
TRIM = {
    'Filterkaffee FK 1': 0.5,
    'Trockenrasur Bass': -1.5,
    'Entsafter Acid': -1.5,
    'Toaster Krümel': -2.5,
    'Tiefkühltruhe Sub': -1.5,
    'Sparschäler Pluck': 4.5,
    'Funktionsbass FM': -1.5,
    'Kompaktanlage Bass': -1.5,
    'Zentralheizung Bass': -5.0,
    'Weniger aber Lead': 0.5,
    'Taschenradio T1': 3.0,
    'Zündung Sync': -2.0,
    'Föhnfrisur Lead': -7.0,
    'Rasierwasser Sinus': -3.5,
    'Gute Form Supersaw': -7.5,
    'Elektropfeife': -4.5,
    'Diaprojektor Solo': 2.0,
    'Wählscheibe FM': -2.0,
    'Schneewittchensarg': -3.5,
    'Sonntagsfahrverbot': 3.5,
    'Wohnlandschaft': -8.0,
    'Orange Hängelampe': -4.5,
    'Teakholz Streicher': -4.0,
    'Cordsofa Wärme': -4.5,
    'Raumteiler Chor': 3.5,
    'Mondbasis Alpha': 3.0,
    'Kristallvase': -7.5,
    'Ferienprospekt': 0.5,
    'Elektroklavier E70': -2.0,
    'Fondueparty Piano': -1.0,
    'Rechenschieber Clav': -4.0,
    'Taschenrechner Bell': -5.0,
    'Wohnzimmerorgel': 2.0,
    'Kirchentag Orgel': 1.0,
    'Studio Vibraphon': -5.0,
    'Spieluhr Mechanik': 1.5,
    'Bundesbahn Bläser': -1.0,
    'Schlagerstreicher': -3.5,
    'Hitparade Synthbrass': 2.0,
    'Intercity Streicher': -5.0,
    'Fernsehballett': 3.0,
    'Lochkarte Arp': 3.0,
    'Rechenzentrum': 2.5,
    'Fernschreiber': -2.0,
    'Mondlandung Arp': 4.5,
    'Zitruspresse Pluck': 3.5,
    'Kugelkopf Pluck': 2.0,
    'Stoppuhr Sequenz': 4.5,
    'Datenträger Klick': 2.0,
    'Hollerith Arp': 2.0,
    'Kassettenrekorder': 1.0,
    'Staubsauger': 4.5,
    'Haartrockner Wind': 4.5,
    'Kurzwelle Störung': 3.0,
    'Weltempfänger': 3.5,
    'Elektrorasierer': -0.5,
    'Kaffeemühle': -4.5,
    'Bildröhre Brummen': -0.5,
    'Wecker Rrring': 3.5,
    'Raumkapsel': -7.0,
}


def amp_curve(v):
    return 0.0 if v <= 0 else 2048 ** ((v / 127) ** 0.3) / 2048


def apply_trim(name, d):
    t = TRIM.get(name, 0.0)
    if not t:
        return
    k = 10 ** (t / 20)
    if d["fx_on"] and d["fx_type"] in (FX["drive1"], FX["drive2"], FX["fold"]):
        d["fx_level"] = clamp((d["fx_level"] + 2) * k - 2)
    else:
        d["amp_gain"] = level(amp_curve(d["amp_gain"]) * k)


for _name, _d in BANK:
    apply_trim(_name, _d)

assert len(BANK) == 64, len(BANK)
assert len({n for n, _ in BANK}) == len(BANK), "duplicate names"


def sy1_text(name, values):
    lines = [name, "color=default", f"ver={VERSION}"]
    for pid, (sy1, lo, hi) in sorted(PARAMS.items(), key=lambda kv: kv[1][0]):
        v = values[pid]
        assert lo <= v <= hi, (name, pid, v)
        lines.append(f"{sy1},{v}")
    return "\r\n".join(lines) + "\r\n"


def main():
    for f in OUT.glob("*"):
        f.unlink()
    for i, (name, values) in enumerate(BANK, 1):
        (OUT / f"{i:03}.sy1").write_bytes(sy1_text(name, values).encode("latin-1"))
    if "--zip" in sys.argv:
        path = Path(sys.argv[sys.argv.index("--zip") + 1])
        with zipfile.ZipFile(path, "w") as z:
            for i, (name, values) in enumerate(BANK, 1):
                z.writestr(f"factory/{i:03}.sy1", sy1_text(name, values).encode("latin-1"))
    print(f"wrote {len(BANK)} patches to {OUT}")


if __name__ == "__main__":
    main()
