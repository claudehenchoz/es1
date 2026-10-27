//! `.sy1` patch files and the embedded ES 1 factory bank.
//!
//! Format:
//! ```text
//! <patch name, max 32 chars>
//! color=<name>          (optional)
//! ver=<int>             (optional, defaults to 100)
//! <sy1 index>,<value>   (repeated, any order, at most 100 entries)
//! ```

use std::io::{Read, Seek};

/// Highest sy1 index (+1) the format uses.
pub const SY1_SLOTS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sy1Patch {
    pub name: String,
    pub color: Option<String>,
    /// File format version (`ver=`), 100 when absent.
    pub version: i32,
    /// Raw values by sy1 index; `None` where the file has no entry.
    pub values: [Option<i32>; SY1_SLOTS],
}

impl Default for Sy1Patch {
    fn default() -> Self {
        Self { name: String::from("init"), color: None, version: 100, values: [None; SY1_SLOTS] }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Empty,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Zip(e) => write!(f, "{e}"),
            Error::Empty => write!(f, "file is empty"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

/// Patch files are Latin-1 (ANSI on Windows). Text that is valid UTF-8 (patches
/// kept in a project's state, or saved by other tools) is taken as UTF-8.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Encode `write_sy1` output as Latin-1 for saving to disk; characters outside it become `?`.
pub fn to_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect()
}

/// C `atoi` semantics: optional whitespace, sign, digits; garbage after is ignored.
fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, rest) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for b in rest.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        v = (v * 10 + (b - b'0') as i64).min(i32::MAX as i64);
    }
    (if neg { -v } else { v }) as i32
}

/// Parse a `.sy1` file. Never fails on malformed lines; they are skipped.
pub fn parse_sy1(bytes: &[u8]) -> Result<Sy1Patch, Error> {
    let text = decode(bytes);
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let name = lines.next().ok_or(Error::Empty)?;
    let mut patch = Sy1Patch { name: name.chars().take(32).collect::<String>().trim().to_string(), ..Default::default() };
    if bytes.is_empty() {
        return Err(Error::Empty);
    }
    let mut count = 0;
    for line in lines {
        if let Some((k, v)) = line.split_once(',') {
            let idx = atoi(k);
            let val = atoi(v);
            if (0..SY1_SLOTS as i32).contains(&idx) {
                let slot = &mut patch.values[idx as usize];
                if slot.is_none() {
                    if count >= SY1_SLOTS {
                        continue;
                    }
                    count += 1;
                }
                *slot = Some(val);
            }
        } else if line.len() >= 6 && line[..6].eq_ignore_ascii_case("color=") {
            patch.color = Some(line[6..].trim().to_string());
        } else if line.len() >= 4 && line[..4].eq_ignore_ascii_case("ver=") {
            patch.version = atoi(&line[4..]);
        }
    }
    Ok(patch)
}

impl Sy1Patch {
    /// Complete current-version parameter values: the template, overlaid with this file's
    /// entries, version-converted and clamped.
    pub fn to_values(&self) -> es1_dsp::Values {
        es1_dsp::params::from_sparse(&self.values[..es1_dsp::params::NUM_SY1], self.version)
    }

    /// Build a current-version patch from engine values.
    pub fn from_values(name: &str, v: &es1_dsp::Values) -> Self {
        let mut p = Sy1Patch { name: name.chars().take(32).collect(), version: es1_dsp::params::CURRENT_VERSION, ..Default::default() };
        for (i, x) in v.iter().enumerate() {
            p.values[i] = Some(*x);
        }
        p
    }
}

/// Serialize in the format's own layout (`%d,%d` lines), so files load in other `.sy1` players.
pub fn write_sy1(p: &Sy1Patch) -> String {
    let mut s = String::new();
    s.push_str(&p.name);
    s.push_str("\r\n");
    s.push_str(&format!("color={}\r\n", p.color.as_deref().unwrap_or("default")));
    s.push_str(&format!("ver={}\r\n", p.version));
    for (i, v) in p.values.iter().enumerate() {
        if let Some(v) = v {
            s.push_str(&format!("{i},{v}\r\n"));
        }
    }
    s
}

/// A named collection of patches (a folder or zip sub-folder).
#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub name: String,
    pub patches: Vec<Sy1Patch>,
}

/// Read every `.sy1` inside a zip archive. Each folder becomes its own bank; patches are
/// sorted by file name (`001.sy1`, `002.sy1`, ...).
pub fn read_zip_banks<R: Read + Seek>(reader: R, archive_name: &str) -> Result<Vec<Bank>, Error> {
    let mut zip = zip::ZipArchive::new(reader)?;
    let mut entries: Vec<(String, String, Sy1Patch)> = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if !f.is_file() {
            continue;
        }
        let path = f.name().replace('\\', "/");
        if !path.to_ascii_lowercase().ends_with(".sy1") {
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        if let Ok(p) = parse_sy1(&buf) {
            let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            entries.push((dir.to_string(), file.to_string(), p));
        }
    }
    entries.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut banks: Vec<Bank> = Vec::new();
    for (dir, _, p) in entries {
        let name = if dir.is_empty() { archive_name.to_string() } else { format!("{archive_name}/{dir}") };
        match banks.last_mut() {
            Some(b) if b.name == name => b.patches.push(p),
            _ => banks.push(Bank { name, patches: vec![p] }),
        }
    }
    Ok(banks)
}

/// Load a single `.sy1`, a `.zip` bank, or a folder of `.sy1` files.
pub fn load_path(path: &std::path::Path) -> Result<Vec<Bank>, Error> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
        let mut files: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sy1")))
            .collect();
        files.sort();
        let patches = files.iter().filter_map(|f| std::fs::read(f).ok()).filter_map(|b| parse_sy1(&b).ok()).collect();
        return Ok(vec![Bank { name: stem, patches }]);
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    if ext.as_deref() == Some("zip") {
        let f = std::fs::File::open(path)?;
        return read_zip_banks(std::io::BufReader::new(f), &stem);
    }
    let bytes = std::fs::read(path)?;
    let p = parse_sy1(&bytes)?;
    Ok(vec![Bank { name: p.name.clone(), patches: vec![p] }])
}

mod factory {
    include!(concat!(env!("OUT_DIR"), "/factory.rs"));
}

/// The factory patches (`presets/factory/`, generated by `tools/factory_presets.py`), in program
/// order.
pub fn factory_bank() -> Bank {
    Bank {
        name: "Factory".to_string(),
        patches: factory::FACTORY.iter().map(|b| parse_sy1(b).expect("factory patch")).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_bank_parses() {
        let b = factory_bank();
        assert_eq!(b.patches.len(), 64);
        for p in &b.patches {
            assert_eq!(p.version, es1_dsp::params::CURRENT_VERSION, "{}", p.name);
            for d in es1_dsp::PARAMS {
                assert!(p.values[d.sy1 as usize].is_some(), "{} missing {}", p.name, d.id);
            }
        }
        assert_eq!(b.patches[0].name, "Filterkaffee FK 1");
        // Latin-1 names keep their umlauts.
        assert!(b.patches.iter().any(|p| p.name == "Föhnfrisur Lead"));
    }

    #[test]
    fn parse_handles_missing_version_and_lf() {
        let p = parse_sy1(b"Lead\n0,2\n14,3\nbogus\n200,1\n").unwrap();
        assert_eq!(p.name, "Lead");
        assert_eq!(p.version, 100);
        assert_eq!(p.values[0], Some(2));
        assert_eq!(p.values[14], Some(3));
        assert_eq!(p.values[1], None);
    }

    #[test]
    fn round_trip() {
        let b = factory_bank();
        for p in &b.patches {
            let s = write_sy1(p);
            let q = parse_sy1(s.as_bytes()).unwrap();
            assert_eq!(p.values, q.values);
            assert_eq!(p.name, q.name);
        }
    }

    #[test]
    fn factory_values_are_in_range() {
        for p in factory_bank().patches {
            let v = p.to_values();
            for d in es1_dsp::PARAMS {
                let x = v[d.sy1 as usize];
                assert_eq!(d.clamp(x), x, "{} {}", p.name, d.id);
            }
            // Re-export at version 113 must load back identically.
            let q = parse_sy1(write_sy1(&Sy1Patch::from_values(&p.name, &v)).as_bytes()).unwrap();
            assert_eq!(q.to_values(), v, "{}", p.name);
        }
    }

    #[test]
    fn zip_banks_load() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            for (path, body) in [
                ("bass/002.sy1", "Two\r\nver=113\r\n0,1\r\n"),
                ("bass/001.sy1", "One\r\nver=113\r\n0,2\r\n"),
                ("pads\\001.sy1", "Pad\r\n14,3\r\n"),
                ("top.sy1", "Top\r\n"),
                ("readme.txt", "not a patch"),
            ] {
                z.start_file(path, opts).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.set_position(0);
        let banks = read_zip_banks(buf, "test").unwrap();
        let names: Vec<_> = banks.iter().map(|b| (b.name.as_str(), b.patches.iter().map(|p| p.name.as_str()).collect::<Vec<_>>())).collect();
        assert_eq!(names, [("test", vec!["Top"]), ("test/bass", vec!["One", "Two"]), ("test/pads", vec!["Pad"])]);
        assert_eq!(banks[2].patches[0].version, 100);
    }
}
