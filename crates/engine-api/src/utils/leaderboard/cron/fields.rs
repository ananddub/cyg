pub const MONTHS: [(&str, u32); 12] = [
    ("jan", 1),
    ("feb", 2),
    ("mar", 3),
    ("apr", 4),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
];

pub const WEEKDAYS: [(&str, u32); 7] = [
    ("sun", 0),
    ("mon", 1),
    ("tue", 2),
    ("wed", 3),
    ("thu", 4),
    ("fri", 5),
    ("sat", 6),
];

pub fn field(spec: &str, min: u32, max: u32, names: &[(&str, u32)]) -> Option<(Vec<bool>, bool)> {
    let mut bits = vec![false; (max - min + 1) as usize];
    let mut star = false;
    for item in spec.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok()?),
            None => (item, 1),
        };
        if step == 0 {
            return None;
        }
        let (low, high) = if range == "*" {
            star = true;
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (value(a, min, max, names)?, value(b, min, max, names)?)
        } else {
            let single = value(range, min, max, names)?;
            (single, if step == 1 { single } else { max })
        };
        if low > high {
            return None;
        }
        let mut current = low;
        while current <= high {
            bits[(current - min) as usize] = true;
            current += step;
        }
    }
    Some((bits, star))
}

fn value(raw: &str, min: u32, max: u32, names: &[(&str, u32)]) -> Option<u32> {
    let named = names
        .iter()
        .find(|(name, _)| raw.len() >= 3 && name.starts_with(&raw[..3].to_ascii_lowercase()));
    let parsed = match named {
        Some((_, value)) => *value,
        None => raw.parse::<u32>().ok()?,
    };
    let folded = if !names.is_empty() && parsed == 7 {
        0
    } else {
        parsed
    };
    if folded < min || folded > max {
        return None;
    }
    Some(folded)
}
