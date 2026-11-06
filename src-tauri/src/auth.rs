//! Logowanie kontem Microsoft (przepływ „device code”): Microsoft → Xbox Live → XSTS → Minecraft.
//! Launcher przechowuje tylko token odświeżania; token dostępu do gry jest pobierany przy każdym starcie.

use crate::download;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEVICE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";

/// Dane sesji przekazywane do gry.
#[derive(Debug, Clone)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    /// "msa" dla konta Microsoft, "legacy" dla trybu offline.
    pub user_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Account {
    refresh_token: String,
    name: String,
    uuid: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub signed_in: bool,
    pub name: Option<String>,
    pub uuid: Option<String>,
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct DeviceResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    refresh_token: Option<String>,
}

fn dir(root: &Path) -> PathBuf {
    root.join("auth")
}

fn client_id_path(root: &Path) -> PathBuf {
    dir(root).join("client_id.txt")
}

fn account_path(root: &Path) -> PathBuf {
    dir(root).join("account.json")
}

pub fn client_id(root: &Path) -> Option<String> {
    let id = std::fs::read_to_string(client_id_path(root)).ok()?;
    let id = id.trim().to_string();
    if valid_client_id(&id) {
        Some(id)
    } else {
        None
    }
}

/// Identyfikator aplikacji Azure to GUID.
pub fn valid_client_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

pub fn set_client_id(root: &Path, id: &str) -> Result<(), String> {
    let id = id.trim();
    if !valid_client_id(id) {
        return Err("Client ID powinien wyglądać jak 8-4-4-4-12 znaków szesnastkowych, np. 00000000-0000-0000-0000-000000000000.".into());
    }
    std::fs::create_dir_all(dir(root)).map_err(|e| e.to_string())?;
    std::fs::write(client_id_path(root), id).map_err(|e| e.to_string())
}

fn load_account(root: &Path) -> Option<Account> {
    let text = std::fs::read_to_string(account_path(root)).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_account(root: &Path, acc: &Account) -> Result<(), String> {
    std::fs::create_dir_all(dir(root)).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(acc).map_err(|e| e.to_string())?;
    std::fs::write(account_path(root), json).map_err(|e| e.to_string())
}

pub fn status(root: &Path) -> Status {
    let acc = load_account(root);
    Status {
        signed_in: acc.is_some(),
        name: acc.as_ref().map(|a| a.name.clone()),
        uuid: acc.as_ref().map(|a| a.uuid.clone()),
        client_id: client_id(root),
    }
}

pub fn logout(root: &Path) {
    let _ = std::fs::remove_file(account_path(root));
}

/// Pierwszy krok: prosi Microsoft o kod, który użytkownik wpisze na stronie.
pub async fn begin(root: &Path) -> Result<(DeviceCode, String, u64), String> {
    let id = client_id(root).ok_or_else(|| "Najpierw wpisz Client ID aplikacji Azure.".to_string())?;
    let client = download::http_client()?;
    let resp = client
        .post(DEVICE_URL)
        .form(&[("client_id", id.as_str()), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Microsoft: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(explain_oauth_error(&body));
    }
    let d: DeviceResp = serde_json::from_value(body).map_err(|e| format!("Błędna odpowiedź Microsoft: {e}"))?;
    Ok((
        DeviceCode {
            user_code: d.user_code,
            verification_uri: d.verification_uri,
            expires_in: d.expires_in,
        },
        d.device_code,
        d.interval,
    ))
}

fn explain_oauth_error(body: &Value) -> String {
    let code = body["error"].as_str().unwrap_or("");
    let desc = body["error_description"].as_str().unwrap_or("");
    if desc.contains("AADSTS70002") || desc.contains("AADSTS7000218") {
        return "W aplikacji Azure włącz „Allow public client flows” (Authentication → Settings).".into();
    }
    if desc.contains("AADSTS700016") {
        return "Microsoft nie zna tego Client ID. Sprawdź, czy skopiowałeś „Application (client) ID”.".into();
    }
    if desc.contains("AADSTS70011") || desc.contains("AADSTS50020") {
        return "Aplikacja Azure musi obsługiwać konta osobiste Microsoft (Personal Microsoft accounts).".into();
    }
    format!("Microsoft odrzucił logowanie ({code}). {desc}")
}

/// Drugi krok: czeka, aż użytkownik zatwierdzi kod. `cancelled` pozwala przerwać oczekiwanie.
pub async fn finish(
    root: &Path,
    device_code: &str,
    interval: u64,
    expires_in: u64,
    cancelled: impl Fn() -> bool,
) -> Result<Status, String> {
    let id = client_id(root).ok_or_else(|| "Brak Client ID.".to_string())?;
    let client = download::http_client()?;
    let mut wait = interval.max(2);
    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in);
    loop {
        if cancelled() {
            return Err("Logowanie anulowane.".into());
        }
        if std::time::Instant::now() > deadline {
            return Err("Kod wygasł. Spróbuj zalogować się ponownie.".into());
        }
        tokio::time::sleep(Duration::from_secs(wait)).await;
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", id.as_str()),
                ("device_code", device_code),
            ])
            .send()
            .await
            .map_err(|e| format!("Brak połączenia z Microsoft: {e}"))?;
        let ok = resp.status().is_success();
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        if ok {
            let tok: TokenResp = serde_json::from_value(body).map_err(|e| e.to_string())?;
            let refresh = tok
                .refresh_token
                .clone()
                .ok_or_else(|| "Microsoft nie zwrócił tokenu odświeżania.".to_string())?;
            let session = minecraft_from_msa(&client, &tok.access_token).await?;
            save_account(
                root,
                &Account {
                    refresh_token: refresh,
                    name: session.name.clone(),
                    uuid: session.uuid.clone(),
                },
            )?;
            return Ok(status(root));
        }
        match body["error"].as_str().unwrap_or("") {
            "authorization_pending" => {}
            "slow_down" => wait += 5,
            "authorization_declined" => return Err("Odrzucono logowanie na stronie Microsoft.".into()),
            "expired_token" => return Err("Kod wygasł. Spróbuj zalogować się ponownie.".into()),
            _ => return Err(explain_oauth_error(&body)),
        }
    }
}

/// Zamienia token Microsoft na sesję Minecrafta (Xbox Live → XSTS → Minecraft → profil).
async fn minecraft_from_msa(client: &reqwest::Client, msa_token: &str) -> Result<Session, String> {
    // 1. Xbox Live
    let xbl: Value = post_json(
        client,
        XBL_URL,
        &json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={msa_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let xbl_token = xbl["Token"].as_str().ok_or("Xbox Live nie zwrócił tokenu.")?.to_string();

    // 2. XSTS
    let xsts: Value = post_json(
        client,
        XSTS_URL,
        &json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
    )
    .await?;
    let xsts_token = xsts["Token"].as_str().ok_or("XSTS nie zwrócił tokenu.")?.to_string();
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or("XSTS nie zwrócił identyfikatora użytkownika.")?
        .to_string();

    // 3. Minecraft
    let mc: Value = post_json(
        client,
        MC_LOGIN_URL,
        &json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }),
    )
    .await?;
    let mc_token = mc["access_token"]
        .as_str()
        .ok_or("Minecraft nie zwrócił tokenu dostępu.")?
        .to_string();

    // 4. Profil (jego brak oznacza konto bez zakupionej gry)
    let resp = client
        .get(MC_PROFILE_URL)
        .bearer_auth(&mc_token)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Minecraft: {e}"))?;
    if resp.status().as_u16() == 404 {
        return Err("To konto Microsoft nie ma zakupionego Minecrafta Java Edition.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Nie udało się pobrać profilu Minecrafta (HTTP {}).", resp.status()));
    }
    let profile: Value = resp.json().await.map_err(|e| e.to_string())?;
    let name = profile["name"].as_str().ok_or("Profil bez nazwy gracza.")?.to_string();
    let id = profile["id"].as_str().ok_or("Profil bez identyfikatora.")?;
    Ok(Session {
        name,
        uuid: id.to_string(),
        access_token: mc_token,
        user_type: "msa".to_string(),
    })
}

async fn post_json(client: &reqwest::Client, url: &str, body: &Value) -> Result<Value, String> {
    let resp = client
        .post(url)
        .header("Accept", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Brak połączenia ({url}): {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        return serde_json::from_str(&text).map_err(|e| format!("Błędna odpowiedź serwera: {e}"));
    }
    Err(explain_service_error(url, status.as_u16(), &text))
}

fn explain_service_error(url: &str, status: u16, text: &str) -> String {
    if url == XSTS_URL {
        let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        match v["XErr"].as_u64() {
            Some(2148916233) => return "To konto Microsoft nie ma profilu Xbox. Utwórz go na xbox.com.".into(),
            Some(2148916235) => return "Xbox Live jest niedostępny w Twoim kraju.".into(),
            Some(2148916236) | Some(2148916237) => return "Konto wymaga weryfikacji wieku na stronie Xbox.".into(),
            Some(2148916238) => return "To konto dziecka: dodaj je do rodziny Microsoft i zezwól na Xbox Live.".into(),
            _ => {}
        }
    }
    if url == MC_LOGIN_URL && (status == 403 || text.contains("Invalid app registration")) {
        return "Minecraft odrzucił Twoją aplikację (Invalid app registration). Mojang musi najpierw zatwierdzić Client ID: formularz https://aka.ms/mce-reviewappid (weryfikacja trwa zwykle do tygodnia).".into();
    }
    format!("Serwer odpowiedział błędem HTTP {status}.")
}

/// Sesja do uruchomienia gry: odświeża logowanie Microsoft albo zwraca None, gdy nikt nie jest zalogowany.
pub async fn session(root: &Path) -> Result<Option<Session>, String> {
    let acc = match load_account(root) {
        Some(a) => a,
        None => return Ok(None),
    };
    let id = client_id(root).ok_or_else(|| "Brak Client ID aplikacji Azure.".to_string())?;
    let client = download::http_client()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", id.as_str()),
            ("refresh_token", acc.refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(|e| format!("Brak połączenia z Microsoft: {e}"))?;
    let ok = resp.status().is_success();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !ok {
        return Err(format!(
            "Logowanie wygasło. Zaloguj się ponownie w Ustawieniach. ({})",
            explain_oauth_error(&body)
        ));
    }
    let tok: TokenResp = serde_json::from_value(body).map_err(|e| e.to_string())?;
    let session = minecraft_from_msa(&client, &tok.access_token).await?;
    // Microsoft może wydać nowy token odświeżania, więc zapisujemy go (i aktualną nazwę gracza).
    save_account(
        root,
        &Account {
            refresh_token: tok.refresh_token.unwrap_or(acc.refresh_token),
            name: session.name.clone(),
            uuid: session.uuid.clone(),
        },
    )?;
    Ok(Some(session))
}
