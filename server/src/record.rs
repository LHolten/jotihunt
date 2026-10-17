use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Response, StatusCode, header::RETRY_AFTER};
use tokio::time::sleep;

use crate::AppState;

impl AppState {
    /// The JSON response of `https://jotihunt.nl/api/2.0/{endpoint}`, also saved if there is a [Recorder].
    ///
    /// When rate limited, this waits until the limit resets before returning the error.
    pub async fn fetch(&self, endpoint: &str) -> anyhow::Result<String> {
        let url = format!("https://jotihunt.nl/api/2.0/{endpoint}");
        let res = self.http.get(url).send().await?;
        if let Some(wait) = retry_after(&res) {
            sleep(wait).await;
        }
        let res = res.error_for_status()?;
        let body = res.text().await?;
        if let Some(record) = &self.record {
            record.save(endpoint, &body);
        }
        Ok(body)
    }
}

/// How long to wait before the rate limit resets, if the response is a 429 with a `Retry-After` in seconds.
fn retry_after(res: &Response) -> Option<Duration> {
    if res.status() != StatusCode::TOO_MANY_REQUESTS {
        return None;
    }
    let secs = res
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    Some(Duration::from_secs(secs))
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
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changes_are_recorded() {
        let dir = std::env::temp_dir().join(format!("jotihunt-{}", uuid::Uuid::new_v4()));
        let recorder = Recorder::new(dir.clone());
        recorder.save("areas", "a");
        recorder.save("areas", "a");
        recorder.save("articles", "a");

        let count = |endpoint| fs::read_dir(dir.join(endpoint)).unwrap().count();
        assert_eq!(count("areas"), 1);
        assert_eq!(count("articles"), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
