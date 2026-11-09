//! A line's blame, read from `git blame --porcelain`: the header line of its commit (the id, the
//! line's numbers), then that commit's fields (`author`, `author-time`, `summary`...) the first
//! time it appears, then the line itself after a tab.

/// The id Git gives lines that are not committed.
const UNCOMMITTED: &str = "0000000000000000000000000000000000000000";

/// Who last changed a line, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blame {
    /// The commit's full id; all zeros for a line not committed yet.
    pub commit: String,
    pub author: String,
    /// Seconds since 1970, UTC.
    pub time: i64,
    /// The author's time zone, as `+0400`.
    pub zone: String,
    pub summary: String,
}

impl Blame {
    /// The first line's blame in porcelain output; `None` for output that has none.
    pub fn parse(porcelain: &str) -> Option<Self> {
        let mut lines = porcelain.lines();
        let header = lines.next()?;
        let commit = header.split(' ').next()?;
        let hex = commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit());
        let numbers = header.split(' ').skip(1).count() >= 2
            && header
                .split(' ')
                .skip(1)
                .all(|number| number.parse::<usize>().is_ok());
        if !hex || !numbers {
            return None;
        }
        let mut blame = Blame {
            commit: commit.to_ascii_lowercase(),
            author: String::new(),
            time: 0,
            zone: String::new(),
            summary: String::new(),
        };
        for line in lines {
            if line.starts_with('\t') {
                break;
            }
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "author" => blame.author = value.to_owned(),
                "author-time" => blame.time = value.parse().unwrap_or(0),
                "author-tz" => blame.zone = value.to_owned(),
                "summary" => blame.summary = value.to_owned(),
                _ => {}
            }
        }
        Some(blame)
    }

    /// Whether the line is not committed yet.
    pub fn is_uncommitted(&self) -> bool {
        self.commit == UNCOMMITTED
    }

    /// The commit's short id.
    pub fn short_id(&self) -> &str {
        &self.commit[..self.commit.len().min(8)]
    }

    /// The day of the commit in its author's time zone, as `2026-11-09`.
    pub fn date(&self) -> String {
        let offset = zone_seconds(&self.zone);
        let days = self.time.saturating_add(offset).div_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// The blame as the call tip shows it.
    pub fn describe(&self) -> String {
        if self.is_uncommitted() {
            return t!("git.not-committed");
        }
        format!(
            "{} {}, {}\n{}",
            self.short_id(),
            self.author,
            self.date(),
            self.summary
        )
    }
}

/// `+0400` or `-0130` as seconds east of UTC; 0 for anything else.
fn zone_seconds(zone: &str) -> i64 {
    let bytes = zone.as_bytes();
    if bytes.len() != 5 || !bytes[1..].iter().all(u8::is_ascii_digit) {
        return 0;
    }
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return 0,
    };
    let hours: i64 = zone[1..3].parse().unwrap_or(0);
    let minutes: i64 = zone[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// The proleptic Gregorian date of day `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
