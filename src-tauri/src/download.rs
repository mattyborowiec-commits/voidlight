use futures::stream::{self, StreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Postęp przygotowania gry, wysyłany do interfejsu.
#[derive(Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
}

pub type Reporter = Arc<dyn Fn(Progress) + Send + Sync>;

pub fn emit(r: &Reporter, stage: &str, done: u64, total: u64) {
    (**r)(Progress {
        stage: stage.to_string(),
        done,
        total,
    });
}

/// Jedno pobranie: adres, miejsce docelowe i opcjonalnie suma SHA-1 oraz rozmiar.
#[derive(Clone)]
pub struct Job {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Voidlight/0.1")
        .build()
        .map_err(|e| e.to_string())
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

/// Szybka kontrola: plik istnieje i ma oczekiwany rozmiar (bez liczenia sumy, żeby start był szybki).
fn is_fresh(job: &Job) -> bool {
    match std::fs::metadata(&job.dest) {
        Ok(m) => match job.size {
            Some(s) => m.len() == s,
            None => m.len() > 0,
        },
        Err(_) => false,
    }
}

async fn fetch_once(client: &reqwest::Client, job: &Job) -> Result<(), String> {
    let resp = client
        .get(&job.url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if let Some(expected) = &job.sha1 {
        if !sha1_hex(&bytes).eq_ignore_ascii_case(expected) {
            return Err("niezgodna suma kontrolna".to_string());
        }
    }
    if let Some(parent) = job.dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let mut tmp = job.dest.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, &job.dest)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn fetch(client: &reqwest::Client, job: &Job) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..3 {
        match fetch_once(client, job).await {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(format!("{}: {}", job.url, last))
}

/// Pobiera jeden plik zawsze od nowa (z trzema próbami i weryfikacją sumy, jeśli jest podana).
pub async fn download_one(client: &reqwest::Client, job: &Job) -> Result<(), String> {
    fetch(client, job).await
}

/// Pobiera tekst (np. JSON) z trzema próbami.
pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let mut last = String::new();
    for _ in 0..3 {
        let attempt: Result<String, String> = async {
            let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            resp.text().await.map_err(|e| e.to_string())
        }
        .await;
        match attempt {
            Ok(t) => return Ok(t),
            Err(e) => last = e,
        }
    }
    Err(format!("{url}: {last}"))
}

/// Pobiera wiele plików równolegle. Pliki, które już są na dysku, są pomijane.
pub async fn download_all(
    client: &reqwest::Client,
    stage: &str,
    jobs: Vec<Job>,
    reporter: &Reporter,
    concurrency: usize,
) -> Result<(), String> {
    let total = jobs.len() as u64;
    emit(reporter, stage, 0, total);

    let client = client.clone();
    let reporter = reporter.clone();
    let stage = stage.to_string();
    let done = Arc::new(AtomicU64::new(0));

    let results: Vec<Result<(), String>> = stream::iter(jobs)
        .map(move |job| {
            let client = client.clone();
            let reporter = reporter.clone();
            let stage = stage.clone();
            let done = done.clone();
            async move {
                let r = if is_fresh(&job) {
                    Ok(())
                } else {
                    fetch(&client, &job).await
                };
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 16 == 0 || d == total {
                    emit(&reporter, &stage, d, total);
                }
                r
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results.into_iter().collect::<Result<Vec<()>, String>>()?;
    Ok(())
}
