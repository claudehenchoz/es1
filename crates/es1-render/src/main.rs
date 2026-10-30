// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Offline renderer for listening tests and engine sanity checks.
//!
//! ```text
//! es1-render [--preset N | --all] [--file path.sy1|.zip] [--chord] [--sr 48000] [--out dir]
//! ```
//! Plays a short phrase (or a held chord with `--chord`) through each selected patch, writes
//! 32-bit float WAVs and prints peak/RMS/NaN statistics. Exits non-zero on NaN/Inf or runaway
//! output.

use es1_dsp::Synth;
use es1_presets::{factory_bank, load_path, Sy1Patch};
use std::io::Write;
use std::path::PathBuf;

fn write_wav(path: &std::path::Path, sr: u32, l: &[f32], r: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let n = l.len() as u32;
    let data = n * 8;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&sr.to_le_bytes())?;
    f.write_all(&(sr * 8).to_le_bytes())?;
    f.write_all(&8u16.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data.to_le_bytes())?;
    for i in 0..l.len() {
        f.write_all(&l[i].to_le_bytes())?;
        f.write_all(&r[i].to_le_bytes())?;
    }
    Ok(())
}

struct Stats {
    peak: f32,
    rms: f32,
    bad: bool,
}

fn render(p: &Sy1Patch, sr: f32, chord: bool) -> (Vec<f32>, Vec<f32>, Stats) {
    let mut s = Synth::new(sr);
    s.load_values(&p.to_values());
    let total = (sr * 6.0) as usize;
    let mut l = vec![0.0f32; total];
    let mut r = vec![0.0f32; total];
    // (time s, note, on)
    let events: Vec<(f32, u8, bool)> = if chord {
        vec![(0.0, 48, true), (0.0, 55, true), (0.0, 60, true), (0.0, 64, true), (3.0, 48, false), (3.0, 55, false), (3.0, 60, false), (3.0, 64, false)]
    } else {
        let mut e = vec![];
        let seq = [60u8, 63, 67, 72, 67, 63, 60, 55];
        for (i, &n) in seq.iter().enumerate() {
            let t = i as f32 * 0.3;
            e.push((t, n, true));
            e.push((t + 0.25, n, false));
        }
        e.push((2.6, 36, true));
        e.push((2.6, 48, true));
        e.push((4.0, 36, false));
        e.push((4.0, 48, false));
        e
    };
    let mut pos = 0usize;
    let block = 128;
    let mut ev = 0;
    while pos < total {
        let t = pos as f32 / sr;
        while ev < events.len() && events[ev].0 <= t {
            let (_, n, on) = events[ev];
            if on {
                s.note_on(n, 100);
            } else {
                s.note_off(n);
            }
            ev += 1;
        }
        let len = block.min(total - pos);
        s.process(&mut l[pos..pos + len], &mut r[pos..pos + len]);
        pos += len;
    }
    let mut peak = 0.0f32;
    let mut acc = 0.0f64;
    let mut bad = false;
    for i in 0..total {
        for v in [l[i], r[i]] {
            if !v.is_finite() {
                bad = true;
            } else {
                peak = peak.max(v.abs());
                acc += (v * v) as f64;
            }
        }
    }
    let rms = (acc / (2 * total) as f64).sqrt() as f32;
    (l, r, Stats { peak, rms, bad: bad || peak > 50.0 })
}

/// Worst-case CPU: 32 voices, each with the 9-oscillator osc 1 detune, sub, FM and LP DL filter.
fn bench(sr: f32) {
    let mut s = Synth::new(sr);
    let p = factory_bank().patches[10].clone(); // Weniger aber Lead
    s.load_values(&p.to_values());
    for (i, v) in [(38, 0.0), (94, 32.0), (73, 1.0), (93, 8.0), (76, 60.0), (95, 80.0), (45, 30.0), (14, 4.0), (25, 0.0), (27, 127.0)] {
        s.set_param(i, v);
    }
    for n in [36u8, 48, 55, 60] {
        s.note_on(n, 100);
    }
    let mut l = vec![0.0f32; 256];
    let mut r = vec![0.0f32; 256];
    s.process(&mut l, &mut r);
    let secs = 10.0;
    let blocks = (sr * secs / 256.0) as usize;
    let t = std::time::Instant::now();
    for _ in 0..blocks {
        s.process(&mut l, &mut r);
    }
    let el = t.elapsed().as_secs_f64();
    println!(
        "{} voices, {:.1} s of audio in {:.3} s: {:.1}x realtime, {:.1}% of one core",
        s.active_voices(),
        secs,
        el,
        secs as f64 / el,
        100.0 * el / secs as f64
    );
}

fn main() {
    if std::env::args().any(|a| a == "--bench") {
        bench(48_000.0);
        return;
    }
    let mut args = std::env::args().skip(1);
    let mut preset: Option<usize> = None;
    let mut file: Option<PathBuf> = None;
    let mut chord = false;
    let mut sr = 48_000.0f32;
    let mut out = PathBuf::from("target/render");
    let mut write = true;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--preset" => preset = args.next().and_then(|v| v.parse().ok()),
            "--all" => preset = None,
            "--file" => file = args.next().map(PathBuf::from),
            "--chord" => chord = true,
            "--sr" => sr = args.next().and_then(|v| v.parse().ok()).unwrap_or(sr),
            "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
            "--no-wav" => write = false,
            _ => {
                eprintln!("unknown argument {a}");
                std::process::exit(2);
            }
        }
    }
    let patches: Vec<Sy1Patch> = match &file {
        Some(f) => load_path(f).expect("load").into_iter().flat_map(|b| b.patches).collect(),
        None => factory_bank().patches,
    };
    if write {
        std::fs::create_dir_all(&out).expect("output dir");
    }
    let mut failures = 0;
    for (i, p) in patches.iter().enumerate() {
        if preset.is_some_and(|n| n != i + 1) {
            continue;
        }
        let (l, r, st) = render(p, sr, chord);
        println!("{:3} {:<24} peak {:6.3}  rms {:6.4}{}", i + 1, p.name, st.peak, st.rms, if st.bad { "  <-- BAD" } else { "" });
        if st.bad {
            failures += 1;
        }
        if write {
            let safe: String = p.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            write_wav(&out.join(format!("{:03}_{safe}.wav", i + 1)), sr as u32, &l, &r).expect("write wav");
        }
    }
    if failures > 0 {
        eprintln!("{failures} patch(es) produced invalid output");
        std::process::exit(1);
    }
}
