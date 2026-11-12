//! Collegamenti WebSocket ai relay della sincronizzazione tra dispositivi (vedi
//! `prep_core::p2p`). Stessa interfaccia su Android/desktop (un thread per relay, TLS con
//! rustls) e nel browser (WebSocket di JavaScript, `web/net.js`); ci si ricollega da soli
//! con attese crescenti finché il collegamento non viene chiuso.

/// Cosa arriva da un relay (l'indice è quello del relay nell'elenco).
#[derive(Debug, Clone)]
pub enum NetEvent {
    Open(usize),
    Frame(String),
    Closed(usize, String),
}

/// Consegna l'evento al controller sul thread dell'interfaccia.
fn deliver(ev: NetEvent) {
    let _ = slint::invoke_from_event_loop(move || crate::app::on_net_event(ev));
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::io::{self, Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use std::sync::Arc;
    use std::time::Duration;
    use tungstenite::Message;

    enum Conn {
        Plain(TcpStream),
        Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
    }

    impl Conn {
        fn tcp(&self) -> &TcpStream {
            match self {
                Conn::Plain(s) => s,
                Conn::Tls(s) => &s.sock,
            }
        }
    }

    impl Read for Conn {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            match self {
                Conn::Plain(s) => s.read(b),
                Conn::Tls(s) => s.read(b),
            }
        }
    }

    impl Write for Conn {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            match self {
                Conn::Plain(s) => s.write(b),
                Conn::Tls(s) => s.write(b),
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            match self {
                Conn::Plain(s) => s.flush(),
                Conn::Tls(s) => s.flush(),
            }
        }
    }

    fn tls_config() -> Arc<rustls::ClientConfig> {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("protocolli TLS")
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }

    fn connect(url: &str) -> Result<tungstenite::WebSocket<Conn>, String> {
        let (tls, rest) = if let Some(r) = url.strip_prefix("wss://") {
            (true, r)
        } else if let Some(r) = url.strip_prefix("ws://") {
            (false, r)
        } else {
            return Err("indirizzo non valido (serve ws:// o wss://)".into());
        };
        let hostport = rest.split('/').next().unwrap_or(rest);
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().map_err(|_| "porta non valida")?),
            None => (hostport.to_string(), if tls { 443 } else { 80 }),
        };
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("indirizzo non trovato")?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        tcp.set_nodelay(true).ok();
        tcp.set_read_timeout(Some(Duration::from_secs(15))).ok();
        let conn = if tls {
            let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
            let c = rustls::ClientConnection::new(tls_config(), name).map_err(|e| e.to_string())?;
            Conn::Tls(Box::new(rustls::StreamOwned::new(c, tcp)))
        } else {
            Conn::Plain(tcp)
        };
        let (ws, _) = tungstenite::client(url, conn).map_err(|e| e.to_string())?;
        // da qui letture brevi: lo stesso thread spedisce e riceve
        ws.get_ref().tcp().set_read_timeout(Some(Duration::from_millis(120))).ok();
        Ok(ws)
    }

    pub struct Link {
        tx: Sender<Option<String>>,
    }

    impl Link {
        pub fn open(idx: usize, url: &str) -> Link {
            let (tx, rx) = channel::<Option<String>>();
            let url = url.to_string();
            std::thread::spawn(move || run(idx, url, rx));
            Link { tx }
        }

        pub fn send(&self, frame: &str) {
            let _ = self.tx.send(Some(frame.to_string()));
        }

        pub fn close(&self) {
            let _ = self.tx.send(None);
        }
    }

    fn run(idx: usize, url: String, rx: Receiver<Option<String>>) {
        let mut wait = 2u64;
        loop {
            match connect(&url) {
                Ok(mut ws) => {
                    wait = 2;
                    deliver(NetEvent::Open(idx));
                    let why = loop {
                        // invio di quello che c'è in coda
                        let mut closed = false;
                        loop {
                            match rx.try_recv() {
                                Ok(Some(f)) => {
                                    if let Err(e) = ws.send(Message::text(f)) {
                                        closed = true;
                                        deliver(NetEvent::Closed(idx, e.to_string()));
                                        break;
                                    }
                                }
                                Ok(None) | Err(TryRecvError::Disconnected) => {
                                    let _ = ws.close(None);
                                    let _ = ws.flush();
                                    return;
                                }
                                Err(TryRecvError::Empty) => break,
                            }
                        }
                        if closed {
                            break String::new();
                        }
                        match ws.read() {
                            Ok(Message::Text(t)) => deliver(NetEvent::Frame(t.to_string())),
                            Ok(Message::Close(_)) => break "chiuso dal relay".to_string(),
                            Ok(_) => {}
                            Err(tungstenite::Error::Io(e))
                                if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) =>
                            {
                                let _ = ws.flush();
                            }
                            Err(e) => break e.to_string(),
                        }
                    };
                    if !why.is_empty() {
                        deliver(NetEvent::Closed(idx, why));
                    }
                }
                Err(e) => deliver(NetEvent::Closed(idx, e)),
            }
            // attesa prima di riprovare, interrotta se il collegamento viene chiuso
            for _ in 0..wait * 10 {
                std::thread::sleep(Duration::from_millis(100));
                match rx.try_recv() {
                    Ok(None) | Err(TryRecvError::Disconnected) => return,
                    _ => {}
                }
            }
            wait = (wait * 2).min(60);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "/web/net.js")]
    extern "C" {
        #[wasm_bindgen(js_name = wsOpen)]
        fn ws_open(id: u32, url: &str, cb: &Closure<dyn FnMut(u32, String)>);
        #[wasm_bindgen(js_name = wsSend)]
        fn ws_send(id: u32, text: &str);
        #[wasm_bindgen(js_name = wsClose)]
        fn ws_close(id: u32);
    }

    thread_local! {
        static NEXT: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
    }

    pub struct Link {
        id: u32,
    }

    impl Link {
        pub fn open(idx: usize, url: &str) -> Link {
            let id = NEXT.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            });
            // kind: 0 aperto, 1 frame, 2 chiuso (testo = motivo)
            let cb = Closure::<dyn FnMut(u32, String)>::new(move |kind: u32, text: String| {
                deliver(match kind {
                    0 => NetEvent::Open(idx),
                    1 => NetEvent::Frame(text),
                    _ => NetEvent::Closed(idx, text),
                })
            });
            ws_open(id, url, &cb);
            cb.forget();
            Link { id }
        }

        pub fn send(&self, frame: &str) {
            ws_send(self.id, frame);
        }

        pub fn close(&self) {
            ws_close(self.id);
        }
    }
}

pub use imp::Link;
