// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! A date the catalog or a mod writes, read as a UTC moment, without a date library.

/// Seconds since 1970-01-01 UTC of `2026-10-12`, `2026-10-12T10:30`, `2026-10-12T10:30:00`, `...Z` or `...+02:00` (a date with no zone is UTC).
pub fn parse_utc(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, time) = match s.split_once(['T', ' ']) {
        Some((d, t)) => (d, Some(t.trim())),
        None => (s, None),
    };
    let mut d = date.split('-');
    let (y, m, day) = (
        d.next()?.parse::<i64>().ok()?,
        d.next()?.parse::<i64>().ok()?,
        d.next()?.parse::<i64>().ok()?,
    );
    if d.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&day) || date.len() > 10 {
        return None;
    }
    let (mut secs, mut offset) = (0i64, 0i64);
    if let Some(t) = time {
        let (clock, zone) = match t.find(['Z', 'z', '+', '-']) {
            Some(at) => (&t[..at], &t[at..]),
            None => (t, ""),
        };
        let mut c = clock.split(':');
        let h = c.next()?.parse::<i64>().ok()?;
        let mi = c.next().unwrap_or("0").parse::<i64>().ok()?;
        let sec = c.next().unwrap_or("0").split('.').next()?.parse::<i64>().ok()?;
        if !(0..24).contains(&h) || !(0..60).contains(&mi) || !(0..60).contains(&sec) {
            return None;
        }
        secs = h * 3600 + mi * 60 + sec;
        if let Some(sign) = zone.chars().next().filter(|c| *c == '+' || *c == '-') {
            let z = &zone[1..];
            let (zh, zm) = z.split_once(':').unwrap_or((z, "0"));
            let off = zh.parse::<i64>().ok()? * 3600 + zm.parse::<i64>().ok()? * 60;
            offset = if sign == '+' { off } else { -off };
        }
    }
    Some(days_from_civil(y, m, day) * 86_400 + secs - offset)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The current time, in seconds since 1970-01-01 UTC.
pub fn now_utc() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_times_read_as_utc() {
        assert_eq!(parse_utc("1970-01-01"), Some(0));
        assert_eq!(parse_utc("2026-10-12T10:30:00Z"), Some(1_791_801_000));
        assert_eq!(parse_utc("2026-10-12T10:30:00"), Some(1_791_801_000));
        assert_eq!(parse_utc("2026-10-12T12:30:00+02:00"), Some(1_791_801_000));
        assert_eq!(parse_utc("2026-10-12T05:30:00-05:00"), Some(1_791_801_000));
        assert_eq!(parse_utc("2026-10-12 10:30"), Some(1_791_801_000));
        assert_eq!(parse_utc("2026-10-12"), Some(1_791_801_000 - 10 * 3600 - 30 * 60));
        assert_eq!(parse_utc("2026-13-01"), None);
        assert_eq!(parse_utc("soon"), None);
        assert_eq!(parse_utc(""), None);
    }
}
