//! Client minimale per l'API REST di GitHub (solo ciò che serve all'app).

#[cfg(not(target_arch = "wasm32"))]
use base64::Engine;
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use ureq::Agent;

#[derive(Debug, Clone)]
pub struct RepoConfig {
    pub api_base: String,
    pub owner: String,
    pub repo: String,
    pub token: String,
}

#[derive(Debug)]
pub enum GhError {
    /// nessuna connessione / timeout
    Network(String),
    /// token non valido o senza permessi
    Auth(u16, String),
    Http(u16, String),
    Parse(String),
}

impl std::fmt::Display for GhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GhError::Network(e) => write!(f, "rete non disponibile: {e}"),
            GhError::Auth(401, m) => {
                write!(f, "token non valido o scaduto (401): creane uno nuovo. {m}")
            }
            GhError::Auth(c, m) => write!(f, "accesso negato ({c}): controlla il token. {m}"),
            GhError::Http(c, m) => write!(f, "errore GitHub {c}: {m}"),
            GhError::Parse(e) => write!(f, "risposta non valida: {e}"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
struct Tree {
    #[serde(default)]
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Deserialize)]
struct RepoInfo {
    default_branch: String,
}

/// File allegato a una release.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub size: u64,
}

/// Release del repository (solo i campi usati dall'aggiornamento dell'app).
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, PartialEq)]
pub enum PutOutcome {
    Created,
    /// il file esiste già (commit già fatto in precedenza)
    AlreadyExists,
}

/// Client dell'API GitHub (rete bloccante: non disponibile nel browser).
#[cfg(not(target_arch = "wasm32"))]
pub struct GitHub {
    cfg: RepoConfig,
    agent: Agent,
}

/// Nel browser non c'è rete bloccante: lo stesso client risponde sempre «non disponibile»
/// (la versione web si aggiorna con il pacchetto e tra i propri dispositivi).
#[cfg(target_arch = "wasm32")]
pub struct GitHub;

#[cfg(target_arch = "wasm32")]
impl GitHub {
    fn off<T>() -> Result<T, GhError> {
        Err(GhError::Network("GitHub non è disponibile nella versione web".into()))
    }
    pub fn new(_: RepoConfig) -> GitHub {
        GitHub
    }
    pub fn default_branch(&self) -> Result<String, GhError> {
        Self::off()
    }
    pub fn branches(&self) -> Result<Vec<String>, GhError> {
        Self::off()
    }
    pub fn tree(&self, _: &str) -> Result<Vec<TreeEntry>, GhError> {
        Self::off()
    }
    pub fn blob(&self, _: &str) -> Result<Vec<u8>, GhError> {
        Self::off()
    }
    pub fn create_file(&self, _: &str, _: &[u8], _: &str, _: &str) -> Result<PutOutcome, GhError> {
        Self::off()
    }
    pub fn create_issue(&self, _: &str, _: &str, _: &[&str]) -> Result<u64, GhError> {
        Self::off()
    }
    pub fn releases(&self, _: u32) -> Result<Vec<Release>, GhError> {
        Self::off()
    }
    pub fn latest_release(&self) -> Result<Release, GhError> {
        Self::off()
    }
    pub fn download_asset(&self, _: u64, _: &std::path::Path) -> Result<(), GhError> {
        Self::off()
    }
}

/// sha di un blob git, per riconoscere i file che abbiamo già
pub fn git_blob_sha(bytes: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.digest().to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn enc_path(p: &str) -> String {
    p.split('/')
        .map(|seg| {
            seg.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{:02X}", b)
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(not(target_arch = "wasm32"))]
impl GitHub {
    pub fn new(cfg: RepoConfig) -> GitHub {
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(40)))
            .http_status_as_error(false)
            .user_agent("tremendouspreparator-app")
            .build()
            .into();
        GitHub { cfg, agent }
    }

    fn url(&self, tail: &str) -> String {
        format!("{}/repos/{}/{}{}", self.cfg.api_base.trim_end_matches('/'), self.cfg.owner, self.cfg.repo, tail)
    }

    fn check(status: u16, body: String) -> Result<String, GhError> {
        match status {
            200..=299 => Ok(body),
            401 | 403 => Err(GhError::Auth(status, short(&body))),
            _ => Err(GhError::Http(status, short(&body))),
        }
    }

    fn get(&self, url: &str, accept: &str) -> Result<(u16, Vec<u8>), GhError> {
        let mut resp = self
            .agent
            .get(url)
            .header("Authorization", &format!("Bearer {}", self.cfg.token))
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| GhError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| GhError::Network(e.to_string()))?;
        Ok((status, bytes))
    }

    pub fn default_branch(&self) -> Result<String, GhError> {
        let (st, b) = self.get(&self.url(""), "application/vnd.github+json")?;
        if st == 404 {
            // per GitHub un repository privato senza accesso "non esiste"
            return Err(GhError::Http(
                404,
                format!(
                    "repository {}/{} non trovato: controlla proprietario e nome, e che il token \
                     abbia accesso a questo repository (Repository access)",
                    self.cfg.owner, self.cfg.repo
                ),
            ));
        }
        let body = Self::check(st, String::from_utf8_lossy(&b).into_owned())?;
        let info: RepoInfo = serde_json::from_str(&body).map_err(|e| GhError::Parse(e.to_string()))?;
        Ok(info.default_branch)
    }

    /// Elenco ricorsivo dei file del branch. Repository vuoto → lista vuota.
    /// Rami del repository (al massimo 100): uno per esame, o più versioni dello stesso esame.
    pub fn branches(&self) -> Result<Vec<String>, GhError> {
        let (st, b) = self.get(&self.url("/branches?per_page=100"), "application/vnd.github+json")?;
        let body = Self::check(st, String::from_utf8_lossy(&b).into_owned())?;
        let v: Vec<serde_json::Value> = serde_json::from_str(&body).map_err(|e| GhError::Parse(e.to_string()))?;
        Ok(v.iter().filter_map(|b| b["name"].as_str().map(str::to_string)).collect())
    }

    pub fn tree(&self, branch: &str) -> Result<Vec<TreeEntry>, GhError> {
        let (st, b) = self
            .get(&self.url(&format!("/git/trees/{}?recursive=1", enc_path(branch))), "application/vnd.github+json")?;
        if st == 409 {
            // repository vuoto
            return Ok(vec![]);
        }
        if st == 404 {
            return Err(GhError::Http(
                404,
                format!("branch \"{branch}\" non trovato (lascia vuoto il campo Branch per usare quello predefinito)"),
            ));
        }
        let body = Self::check(st, String::from_utf8_lossy(&b).into_owned())?;
        let t: Tree = serde_json::from_str(&body).map_err(|e| GhError::Parse(e.to_string()))?;
        if t.truncated {
            return Err(GhError::Parse("albero troncato (troppi file)".into()));
        }
        Ok(t.tree)
    }

    pub fn blob(&self, sha: &str) -> Result<Vec<u8>, GhError> {
        let (st, b) = self.get(&self.url(&format!("/git/blobs/{sha}")), "application/vnd.github.raw+json")?;
        if (200..300).contains(&st) {
            Ok(b)
        } else {
            Self::check(st, String::from_utf8_lossy(&b).into_owned()).map(|_| vec![])
        }
    }

    /// Crea un file con un commit sul branch indicato.
    pub fn create_file(&self, path: &str, bytes: &[u8], message: &str, branch: &str) -> Result<PutOutcome, GhError> {
        let body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
            "branch": branch,
        })
        .to_string();
        let mut resp = self
            .agent
            .put(&self.url(&format!("/contents/{}", enc_path(path))))
            .header("Authorization", &format!("Bearer {}", self.cfg.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Content-Type", "application/json")
            .send(body.as_bytes())
            .map_err(|e| GhError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        match status {
            200 | 201 => Ok(PutOutcome::Created),
            // "sha wasn't supplied": il file c'è già → già committato
            422 if text.contains("sha") => Ok(PutOutcome::AlreadyExists),
            409 if text.contains("empty") => {
                Err(GhError::Http(status, "repository vuoto: serve almeno un commit sul branch".into()))
            }
            _ => Self::check(status, text).map(|_| PutOutcome::Created),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl GitHub {
    fn post_json(&self, tail: &str, body: &serde_json::Value) -> Result<String, GhError> {
        let mut resp = self
            .agent
            .post(&self.url(tail))
            .header("Authorization", &format!("Bearer {}", self.cfg.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Content-Type", "application/json")
            .send(body.to_string().as_bytes())
            .map_err(|e| GhError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().read_to_string().unwrap_or_default();
        Self::check(status, text)
    }

    /// Apre una issue; restituisce il numero. Serve il permesso «Issues: Read and write».
    pub fn create_issue(&self, title: &str, body: &str, labels: &[&str]) -> Result<u64, GhError> {
        let text = self.post_json("/issues", &serde_json::json!({ "title": title, "body": body, "labels": labels }))?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| GhError::Parse(e.to_string()))?;
        v.get("number").and_then(|n| n.as_u64()).ok_or_else(|| GhError::Parse("numero della issue mancante".into()))
    }

    /// Le release più recenti (al massimo `n`, dalla più nuova).
    pub fn releases(&self, n: u32) -> Result<Vec<Release>, GhError> {
        let (st, b) = self.get(&self.url(&format!("/releases?per_page={n}")), "application/vnd.github+json")?;
        let body = Self::check(st, String::from_utf8_lossy(&b).into_owned())?;
        serde_json::from_str(&body).map_err(|e| GhError::Parse(e.to_string()))
    }

    /// Ultima release pubblicata (non bozza, non pre-release).
    pub fn latest_release(&self) -> Result<Release, GhError> {
        let (st, b) = self.get(&self.url("/releases/latest"), "application/vnd.github+json")?;
        let body = Self::check(st, String::from_utf8_lossy(&b).into_owned())?;
        serde_json::from_str(&body).map_err(|e| GhError::Parse(e.to_string()))
    }

    /// Scarica un allegato di release in `dest` (passando da un file temporaneo). GitHub
    /// risponde con un rinvio a un altro host: il token non viene inoltrato.
    pub fn download_asset(&self, id: u64, dest: &std::path::Path) -> Result<(), GhError> {
        let mut resp = self
            .agent
            .get(&self.url(&format!("/releases/assets/{id}")))
            .header("Authorization", &format!("Bearer {}", self.cfg.token))
            .header("Accept", "application/octet-stream")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| GhError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let text = resp.body_mut().read_to_string().unwrap_or_default();
            return Self::check(status, text).map(|_| ());
        }
        let tmp = dest.with_extension("part");
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| GhError::Network(e.to_string()))?;
        }
        let mut file = std::fs::File::create(&tmp).map_err(|e| GhError::Network(e.to_string()))?;
        let mut reader = resp.body_mut().with_config().limit(512 * 1024 * 1024).reader();
        std::io::copy(&mut reader, &mut file).map_err(|e| GhError::Network(e.to_string()))?;
        drop(file);
        std::fs::rename(&tmp, dest).map_err(|e| GhError::Network(e.to_string()))
    }
}

/// Versione da un tag di release (`v0.2.71` → `0.2.71`).
pub fn tag_version(tag: &str) -> &str {
    tag.trim().trim_start_matches('v')
}

/// versionCode Android di una versione `a.b.c` (lo stesso calcolo di cargo-apk).
pub fn version_code(version: &str) -> Option<u32> {
    let mut it = version.split(['.', '-', '+']).map(|p| p.parse::<u8>().ok());
    let (a, b, c) = (it.next()??, it.next()??, it.next()??);
    Some(1 << 24 | (a as u32) << 16 | (b as u32) << 8 | c as u32)
}

#[cfg(not(target_arch = "wasm32"))]
fn short(s: &str) -> String {
    let msg = serde_json::from_str::<serde_json::Value>(s)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|m| m.to_string()))
        .unwrap_or_else(|| s.chars().take(160).collect());
    msg
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_versions() {
        use super::*;
        assert_eq!(tag_version("v0.2.71"), "0.2.71");
        assert!(version_code("0.3.0").unwrap() > version_code("0.2.255").unwrap());
        assert!(version_code("0.2.72+72-abc").unwrap() > version_code("0.2.71").unwrap());
        assert_eq!(version_code("x"), None);
        let r: Release =
            serde_json::from_str(r#"{"tag_name":"v0.2.80","assets":[{"id":7,"name":"SemestreAperto.apk","size":10}]}"#)
                .unwrap();
        assert_eq!(r.assets[0].id, 7);
    }

    #[test]
    fn blob_sha() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(super::git_blob_sha(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn path_encoding() {
        assert_eq!(super::enc_path("progress/a b.json"), "progress/a%20b.json");
    }
}
