//! Client minimale per l'API REST di GitHub (solo ciò che serve all'app).

use base64::Engine;
use serde::Deserialize;
use std::time::Duration;
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

#[derive(Debug, Deserialize)]
struct Tree {
    #[serde(default)]
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct RepoInfo {
    default_branch: String,
}

#[derive(Debug, PartialEq)]
pub enum PutOutcome {
    Created,
    /// il file esiste già (commit già fatto in precedenza)
    AlreadyExists,
}

pub struct GitHub {
    cfg: RepoConfig,
    agent: Agent,
}

/// sha di un blob git, per riconoscere i file che abbiamo già
pub fn git_blob_sha(bytes: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.digest().to_string()
}

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
    fn blob_sha() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(super::git_blob_sha(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn path_encoding() {
        assert_eq!(super::enc_path("progress/a b.json"), "progress/a%20b.json");
    }
}
