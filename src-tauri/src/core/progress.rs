//! The progress event every stage emits, and parsers for what the Tools print.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Info,
    Download,
    Transcode,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    /// 0 to 100, or `None` when unknown.
    pub percent: Option<f64>,
    /// Bytes per second.
    pub speed: Option<f64>,
    /// Seconds remaining.
    pub eta: Option<f64>,
    pub downloaded_bytes: Option<f64>,
    pub total_bytes: Option<f64>,
}

/// `NA`, empty and non-numeric values are unknown.
fn num(v: Option<&str>) -> Option<f64> {
    v?.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// A line from yt-dlp's `--progress-template`:
/// `GHPROGRESS <downloaded> <total> <total estimate> <speed> <eta>`.
fn parse_yt_dlp(line: &str) -> Option<Progress> {
    let mut fields = line.strip_prefix("GHPROGRESS ")?.split(' ');
    let downloaded_bytes = num(fields.next());
    let total = num(fields.next());
    let estimate = num(fields.next());
    let total_bytes = total.or(estimate);
    let percent = downloaded_bytes
        .zip(total_bytes)
        .filter(|&(_, total)| total > 0.0)
        .map(|(done, total)| (done / total * 100.0).clamp(0.0, 100.0));

    Some(Progress {
        stage: Stage::Download,
        percent,
        speed: num(fields.next()),
        eta: num(fields.next()),
        downloaded_bytes,
        total_bytes,
    })
}

/// An aria2c console readout, which starts with `[#`, e.g. `[#a1b2c3 12MiB/345MiB(3%) CN:8 DL:5.2MiB ETA:1m2s]`.
/// aria2c reports binary units: `K`, `M`, `G` and `T` all mean powers of 1024, with or
/// without `iB`.
fn parse_aria2c(line: &str) -> Option<Progress> {
    // Only readouts: other lines can hold a title such as "Sale (50%) off".
    if !line.trim_start().starts_with("[#") {
        return None;
    }
    let pct_end = line.find("%)")?;
    let open = line[..pct_end].rfind('(')?;
    let percent: f64 = line[open + 1..pct_end].parse::<u32>().ok()?.into();

    let (downloaded_bytes, total_bytes) = line[..open]
        .rsplit(' ')
        .next()
        .and_then(|sizes| sizes.split_once('/'))
        .map_or((None, None), |(done, total)| {
            (parse_size(done), parse_size(total))
        });

    Some(Progress {
        stage: Stage::Download,
        percent: Some(percent.clamp(0.0, 100.0)),
        speed: field(line, "DL:", |c| c.is_ascii_alphanumeric() || c == '.').and_then(parse_size),
        eta: field(line, "ETA:", |c| c.is_ascii_alphanumeric()).and_then(parse_duration),
        downloaded_bytes,
        total_bytes,
    })
}

/// Turns yt-dlp's output into one progress for the whole download. With "best" quality yt-dlp
/// downloads the video stream and then the audio stream, each from 0 to 100%, and then merges
/// them, so each stream's percent is scaled to its share and the merge has no percent.
#[derive(Default)]
pub(super) struct YtDlpProgress {
    /// Streams yt-dlp said it will download; 0 until it says.
    streams: u32,
    /// The stream being downloaded, from 1.
    current: u32,
}

impl YtDlpProgress {
    pub(super) fn line(&mut self, line: &str) -> Option<Progress> {
        if let Some(progress) = parse_yt_dlp(line).or_else(|| parse_aria2c(line)) {
            return Some(self.scaled(progress));
        }
        // `[info] <id>: Downloading 1 format(s): 395+251`
        if let Some((_, ids)) = line
            .strip_prefix("[info] ")
            .and_then(|rest| rest.split_once(" format(s): "))
        {
            self.streams = ids.split('+').count() as u32;
            self.current = 0;
        } else if line.starts_with("[download] Destination:") {
            self.current += 1;
        } else if line.starts_with("[Merger]") {
            return Some(Progress::at(Stage::Download, None));
        }
        None
    }

    fn scaled(&self, mut progress: Progress) -> Progress {
        if self.streams > 1 && self.current >= 1 {
            let done = f64::from(self.current.min(self.streams) - 1);
            let share = f64::from(self.streams);
            progress.percent = progress.percent.map(|p| (done + p / 100.0) / share * 100.0);
            // Sizes and ETA are the current stream's, not the whole download's.
            progress.downloaded_bytes = None;
            progress.total_bytes = None;
            progress.eta = None;
        }
        progress
    }
}

/// The text after `key` for as long as `keep` holds.
fn field<'a>(line: &'a str, key: &str, keep: fn(char) -> bool) -> Option<&'a str> {
    let rest = &line[line.find(key)? + key.len()..];
    Some(&rest[..rest.find(|c| !keep(c)).unwrap_or(rest.len())])
}

/// `12MiB` to bytes.
fn parse_size(s: &str) -> Option<f64> {
    let unit_at = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let value: f64 = s[..unit_at].parse().ok()?;
    let factor = match s[unit_at..].chars().next().map(|c| c.to_ascii_uppercase()) {
        Some('K') => 1024.0,
        Some('M') => 1024.0 * 1024.0,
        Some('G') => 1024.0 * 1024.0 * 1024.0,
        Some('T') => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    Some((value * factor).round())
}

/// `1h2m3s` to seconds.
fn parse_duration(s: &str) -> Option<f64> {
    let mut total = None;
    let mut n = 0.0;
    for c in s.chars() {
        match c {
            '0'..='9' => n = n * 10.0 + f64::from(c as u8 - b'0'),
            'h' | 'm' | 's' => {
                let unit = match c {
                    'h' => 3600.0,
                    'm' => 60.0,
                    _ => 1.0,
                };
                *total.get_or_insert(0.0) += n * unit;
                n = 0.0;
            }
            _ => n = 0.0,
        }
    }
    total
}

/// Turns the lines of `ffmpeg -progress pipe:1 -nostats` into events. ffmpeg prints a block of
/// `key=value` lines and ends it with `progress=continue` (or `end`), so one event is
/// emitted per block. Without a known `duration` (seconds) there is no percent or ETA.
pub(super) struct FfmpegProgress {
    duration: Option<f64>,
    out_time_secs: Option<f64>,
    speed: Option<f64>,
}

impl FfmpegProgress {
    pub(super) fn new(duration: Option<f64>) -> Self {
        FfmpegProgress {
            duration: duration.filter(|d| *d > 0.0),
            out_time_secs: None,
            speed: None,
        }
    }

    pub(super) fn line(&mut self, line: &str) -> Option<Progress> {
        let (key, value) = line.split_once('=')?;
        let value = value.trim();
        match key.trim() {
            // Both are microseconds, despite the name of the second (an ffmpeg quirk).
            // `N/A` early in a run keeps the last known position.
            "out_time_us" | "out_time_ms" => {
                if let Some(us) = num(Some(value)) {
                    self.out_time_secs = Some(us / 1e6);
                }
            }
            "speed" => self.speed = value.strip_suffix('x').and_then(|v| num(Some(v.trim()))),
            "progress" => return Some(self.event()),
            _ => {}
        }
        None
    }

    fn event(&self) -> Progress {
        let known = self.duration.zip(self.out_time_secs);
        let mut progress = Progress::at(
            Stage::Transcode,
            known.map(|(total, at)| (at / total * 100.0).clamp(0.0, 100.0)),
        );
        progress.eta = known
            .zip(self.speed.filter(|s| *s > 0.0))
            .map(|((total, at), speed)| ((total - at) / speed).max(0.0));
        progress
    }
}

impl Progress {
    pub(super) fn at(stage: Stage, percent: Option<f64>) -> Self {
        Progress {
            stage,
            percent,
            speed: None,
            eta: None,
            downloaded_bytes: None,
            total_bytes: None,
        }
    }
}
