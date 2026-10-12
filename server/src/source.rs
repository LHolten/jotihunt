use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// The endpoints of the jotihunt.nl API that are used.
const ENDPOINTS: [&str; 3] = ["areas", "articles", "subscriptions"];

/// Where the data of the jotihunt.nl API comes from.
pub enum Source {
    /// The real API, the responses are also saved if there is a [Recorder].
    Api { record: Option<Recorder> },
    /// Responses that were saved during an earlier hunt.
    Replay(Replay),
}

impl Source {
    /// The JSON response of `https://jotihunt.nl/api/2.0/{endpoint}`.
    pub async fn fetch(&self, endpoint: &str) -> anyhow::Result<String> {
        match self {
            Source::Api { record } => {
                let url = format!("https://jotihunt.nl/api/2.0/{endpoint}");
                let res = reqwest::get(url).await?.error_for_status()?;
                let body = res.text().await?;
                if let Some(record) = record {
                    record.save(endpoint, &body);
                }
                Ok(body)
            }
            Source::Replay(replay) => replay.fetch_at(endpoint, replay.now()),
        }
    }

    /// How long to wait between requests that are normally `normal` apart.
    /// A sped up replay asks more often, but at most once a second.
    pub fn interval(&self, normal: Duration) -> Duration {
        match self {
            Source::Api { .. } => normal,
            Source::Replay(replay) => normal
                .div_f64(replay.speed)
                .max(Duration::from_secs(1))
                .min(normal),
        }
    }
}

/// Saves responses as `{dir}/{endpoint}/{unix time in milliseconds}.json`.
pub struct Recorder {
    dir: PathBuf,
    /// The last saved response of every endpoint, only changes are saved.
    last: Mutex<HashMap<String, String>>,
}

impl Recorder {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            last: Mutex::default(),
        }
    }

    /// Errors are only printed, recording should never get in the way of the hunt.
    fn save(&self, endpoint: &str, body: &str) {
        let mut last = self.last.lock().unwrap();
        if last.get(endpoint).is_some_and(|last| last == body) {
            return;
        }
        let millis = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
        let dir = self.dir.join(endpoint);
        let res = fs::create_dir_all(&dir)
            .and_then(|()| fs::write(dir.join(format!("{millis}.json")), body));
        match res {
            Ok(()) => {
                last.insert(endpoint.to_owned(), body.to_owned());
            }
            Err(err) => println!("error recording {endpoint}: {err}"),
        }
    }
}

/// Plays the responses saved by a [Recorder] back as if the hunt starts now.
///
/// All timestamps in the responses are moved, so that the moment the recording
/// started becomes the moment the replay started.
pub struct Replay {
    /// The responses of every endpoint with the time they were saved, oldest first.
    snapshots: HashMap<String, Vec<(OffsetDateTime, PathBuf)>>,
    /// When the oldest response was saved.
    recorded_start: OffsetDateTime,
    replay_start: OffsetDateTime,
    started: Instant,
    /// How many times faster than real time the recording is played.
    speed: f64,
}

impl Replay {
    pub fn open(dir: PathBuf, speed: f64) -> anyhow::Result<Self> {
        if !(speed.is_finite() && speed > 0.0) {
            bail!("the replay speed has to be more than zero");
        }
        let mut snapshots = HashMap::new();
        for endpoint in ENDPOINTS {
            let dir = dir.join(endpoint);
            let mut files = vec![];
            for entry in fs::read_dir(&dir).with_context(|| format!("can not read {dir:?}"))? {
                let path = entry?.path();
                if path.extension() != Some("json".as_ref()) {
                    continue;
                }
                let stem = path.file_stem().and_then(|stem| stem.to_str());
                let Some(millis) = stem.and_then(|stem| stem.parse::<i128>().ok()) else {
                    continue;
                };
                let saved_at = OffsetDateTime::from_unix_timestamp_nanos(millis * 1_000_000)
                    .with_context(|| format!("{path:?} is not named after a time"))?;
                files.push((saved_at, path));
            }
            if files.is_empty() {
                bail!("there are no recordings in {dir:?}");
            }
            files.sort();
            snapshots.insert(endpoint.to_owned(), files);
        }

        let first = snapshots.values().map(|files| files[0].0).min();
        let last = snapshots.values().map(|files| files[files.len() - 1].0);
        let recorded_start = first.expect("there are endpoints");
        let recorded = last.max().expect("there are endpoints") - recorded_start;
        println!(
            "replaying {:.0} minutes of recordings in {:.1} minutes",
            recorded.as_seconds_f64() / 60.0,
            recorded.as_seconds_f64() / 60.0 / speed,
        );

        Ok(Self {
            snapshots,
            recorded_start,
            // whole seconds, just like the timestamps of the API
            replay_start: OffsetDateTime::now_utc().replace_nanosecond(0).unwrap(),
            started: Instant::now(),
            speed,
        })
    }

    /// The moment in the recording that is played right now.
    fn now(&self) -> OffsetDateTime {
        self.recorded_start + self.started.elapsed().mul_f64(self.speed)
    }

    /// The response of `endpoint` as it was at `now` in the recording.
    fn fetch_at(&self, endpoint: &str, now: OffsetDateTime) -> anyhow::Result<String> {
        let files = self
            .snapshots
            .get(endpoint)
            .with_context(|| format!("{endpoint} is not recorded"))?;
        // endpoints are first saved moments apart, so before that the first response is used
        let newer = files.partition_point(|(saved_at, _)| *saved_at <= now);
        let path = &files[newer.saturating_sub(1)].1;

        let body = fs::read_to_string(path).with_context(|| format!("can not read {path:?}"))?;
        let mut json = serde_json::from_str(&body)?;
        self.move_timestamps(&mut json);
        Ok(json.to_string())
    }

    /// Replaces every timestamp with the time at which it is reached in the replay.
    fn move_timestamps(&self, json: &mut Value) {
        match json {
            Value::String(text) => {
                if let Ok(time) = OffsetDateTime::parse(text, &Rfc3339) {
                    let moved = self.replay_start + (time - self.recorded_start) / self.speed;
                    let moved = moved.replace_nanosecond(0).unwrap();
                    *text = moved.format(&Rfc3339).unwrap();
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.move_timestamps(item)),
            Value::Object(fields) => fields.values_mut().for_each(|v| self.move_timestamps(v)),
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jotihunt-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn only_changes_are_recorded() {
        let dir = temp_dir();
        let recorder = Recorder::new(dir.clone());
        recorder.save("areas", "a");
        recorder.save("areas", "a");
        recorder.save("articles", "a");

        let count = |endpoint| fs::read_dir(dir.join(endpoint)).unwrap().count();
        assert_eq!(count("areas"), 1);
        assert_eq!(count("articles"), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replay_follows_the_recording() {
        let dir = temp_dir();
        let save = |endpoint: &str, millis: u64, body: &str| {
            fs::create_dir_all(dir.join(endpoint)).unwrap();
            fs::write(dir.join(endpoint).join(format!("{millis}.json")), body).unwrap();
        };
        // recorded from 2026-10-17 10:00:00 until 10:02:00
        let start = 1_792_231_200_000;
        save("subscriptions", start, r#"{"data":[]}"#);
        save("articles", start + 500, r#"{"data":[]}"#);
        let area = |status: &str, updated_at: &str| {
            format!(
                r#"{{"data":[{{"name":"Alpha","status":"{status}","updated_at":"{updated_at}"}}]}}"#
            )
        };
        save("areas", start, &area("red", "2026-10-17T09:58:00.000000Z"));
        save(
            "areas",
            start + 120_000,
            &area("green", "2026-10-17T10:02:00.000000Z"),
        );

        let replay = Replay::open(dir.clone(), 60.0).unwrap();
        let at = |seconds| replay.recorded_start + Duration::from_secs(seconds);
        let moved = |seconds| {
            let time = replay.replay_start + time::Duration::seconds(seconds);
            time.format(&Rfc3339).unwrap()
        };

        // two minutes before the recording is two seconds before the replay
        let red = area("red", &moved(-2));
        assert_eq!(replay.fetch_at("areas", at(0)).unwrap(), red);
        assert_eq!(replay.fetch_at("areas", at(119)).unwrap(), red);
        let green = area("green", &moved(2));
        assert_eq!(replay.fetch_at("areas", at(120)).unwrap(), green);
        assert_eq!(replay.fetch_at("areas", at(9999)).unwrap(), green);
        // saved just after the start of the recording
        assert!(replay.fetch_at("articles", at(0)).is_ok());

        let source = Source::Replay(replay);
        let seconds = |normal| source.interval(Duration::from_secs(normal)).as_secs();
        assert_eq!(seconds(3600), 60);
        assert_eq!(seconds(5), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
