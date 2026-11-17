//! Collegamento diretto con un dispositivo della stanza (WebRTC DataChannel).
//!
//! I relay servono solo per l'appuntamento: offerta e risposta SDP viaggiano cifrate come
//! messaggi `Rtc` del protocollo (`prep_core::p2p`), poi i dati passano direttamente tra i
//! due dispositivi. Per scoprire il proprio indirizzo pubblico si interroga un server STUN
//! (riceve solo la richiesta, nessun dato). Se il collegamento non riesce (alcune reti lo
//! bloccano) l'app torna ai relay, a meno che l'utente non abbia scelto «solo diretto».
//!
//! Android e desktop: un thread per collegamento con [str0m] (WebRTC in puro Rust, senza
//! OpenSSL) e un socket UDP. Browser: `RTCPeerConnection` di JavaScript (`web/rtc.js`).

/// Server STUN pubblici usati per scoprire l'indirizzo esterno.
pub const STUN: &[&str] = &["stun.l.google.com:19302", "stun.cloudflare.com:3478"];

/// Cosa succede su un collegamento diretto.
#[derive(Debug)]
pub enum DirectEvent {
    /// SDP da mandare all'altro dispositivo: ("offer" | "answer", sdp)
    Signal(String, String),
    Open,
    Data(Vec<u8>),
    Closed(String),
}

fn deliver(sess: usize, peer: String, ev: DirectEvent) {
    let _ = slint::invoke_from_event_loop(move || crate::app::on_direct_event(sess, peer, ev));
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
/// Toglie dall'SDP i candidati «nascosti» (indirizzi `.local` dei browser), che senza mDNS
/// non si possono raggiungere: restano quelli con indirizzo vero.
pub fn strip_mdns(sdp: &str) -> String {
    let mut out: String = sdp
        .split("\r\n")
        .flat_map(|l| l.split('\n'))
        .filter(|l| !(l.starts_with("a=candidate") && l.contains(".local ")))
        .collect::<Vec<_>>()
        .join("\r\n");
    if !out.ends_with("\r\n") {
        out.push_str("\r\n");
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use std::time::{Duration, Instant};
    use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
    use str0m::channel::ChannelId;
    use str0m::net::{Protocol, Receive};
    use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

    enum Cmd {
        Remote(String, String),
        Send(Vec<u8>),
        Close,
    }

    pub struct Conn {
        tx: Sender<Cmd>,
    }

    impl Conn {
        /// Apre un collegamento con `peer`; chi `initiator` prepara l'offerta.
        pub fn open(sess: usize, peer: &str, initiator: bool) -> Conn {
            let (tx, rx) = channel();
            let peer = peer.to_string();
            std::thread::spawn(move || {
                let why = match run(sess, &peer, initiator, rx) {
                    Ok(()) => String::new(),
                    Err(e) => e,
                };
                deliver(sess, peer, DirectEvent::Closed(why));
            });
            Conn { tx }
        }

        /// SDP arrivato dall'altro dispositivo.
        pub fn signal(&self, kind: &str, sdp: &str) {
            let _ = self.tx.send(Cmd::Remote(kind.to_string(), sdp.to_string()));
        }

        pub fn send(&self, bytes: Vec<u8>) {
            let _ = self.tx.send(Cmd::Send(bytes));
        }

        pub fn close(&self) {
            let _ = self.tx.send(Cmd::Close);
        }
    }

    /// Indirizzo locale della rete principale (senza mandare pacchetti: `connect` su UDP
    /// sceglie solo il percorso).
    fn local_ip() -> IpAddr {
        UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| {
                s.connect("8.8.8.8:80")?;
                s.local_addr()
            })
            .map(|a| a.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    /// Richiesta STUN «binding»: l'indirizzo con cui il server vede questo socket.
    fn stun_mapped(sock: &UdpSocket, server: &str) -> Option<SocketAddr> {
        let addr = server.to_socket_addrs().ok()?.find(|a| a.is_ipv4())?;
        let mut tid = [0u8; 12];
        tid.copy_from_slice(&prep_core::p2p::random_id().as_bytes()[..12]);
        let mut req = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
        req.extend_from_slice(&tid);
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut buf = [0u8; 1024];
        sock.send_to(&req, addr).ok()?;
        while Instant::now() < deadline {
            sock.set_read_timeout(Some(Duration::from_millis(200))).ok()?;
            let Ok((n, _)) = sock.recv_from(&mut buf) else { continue };
            let b = &buf[..n];
            if n < 20 || b[0..2] != [0x01, 0x01] || b[8..20] != tid {
                continue;
            }
            let mut i = 20;
            while i + 4 <= n {
                let kind = u16::from_be_bytes([b[i], b[i + 1]]);
                let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
                let v = b.get(i + 4..i + 4 + len)?;
                // XOR-MAPPED-ADDRESS (0x0020) o MAPPED-ADDRESS (0x0001), IPv4
                if (kind == 0x0020 || kind == 0x0001) && len >= 8 && v[1] == 0x01 {
                    let xor = kind == 0x0020;
                    let port = u16::from_be_bytes([v[2], v[3]]) ^ if xor { 0x2112 } else { 0 };
                    let mut ip = [v[4], v[5], v[6], v[7]];
                    if xor {
                        for (k, m) in ip.iter_mut().zip([0x21, 0x12, 0xA4, 0x42]) {
                            *k ^= m;
                        }
                    }
                    return Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port));
                }
                i += 4 + len.div_ceil(4) * 4;
            }
        }
        None
    }

    fn run(sess: usize, peer: &str, initiator: bool, rx: Receiver<Cmd>) -> Result<(), String> {
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        let port = sock.local_addr().map_err(|e| e.to_string())?.port();
        let ip = local_ip();
        let host = SocketAddr::new(ip, port);
        let mut rtc = Rtc::builder().build(Instant::now());
        let mut locals = vec![host];
        if let Ok(c) = Candidate::host(host, "udp") {
            rtc.add_local_candidate(c);
        }
        if !ip.is_loopback() {
            let lo = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
            if let Ok(c) = Candidate::host(lo, "udp") {
                rtc.add_local_candidate(c);
                locals.push(lo);
            }
        }
        for s in STUN {
            if let Some(mapped) = stun_mapped(&sock, s) {
                if mapped != host {
                    if let Ok(c) = Candidate::server_reflexive(mapped, host, "udp") {
                        rtc.add_local_candidate(c);
                    }
                }
                break;
            }
        }
        let mut pending: Option<SdpPendingOffer> = None;
        if initiator {
            let mut api = rtc.sdp_api();
            api.add_channel("tp".into());
            let (offer, p) = api.apply().ok_or("offerta non creata")?;
            pending = Some(p);
            deliver(sess, peer.to_string(), DirectEvent::Signal("offer".into(), offer.to_sdp_string()));
        }
        let mut cid: Option<ChannelId> = None;
        let mut outq: std::collections::VecDeque<Vec<u8>> = Default::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut buf = vec![0u8; 2048];
        loop {
            loop {
                match rx.try_recv() {
                    Ok(Cmd::Remote(kind, sdp)) => {
                        let sdp = strip_mdns(&sdp);
                        if kind == "offer" && !initiator {
                            let offer = SdpOffer::from_sdp_string(&sdp).map_err(|e| e.to_string())?;
                            let answer = rtc.sdp_api().accept_offer(offer).map_err(|e| e.to_string())?;
                            deliver(
                                sess,
                                peer.to_string(),
                                DirectEvent::Signal("answer".into(), answer.to_sdp_string()),
                            );
                        } else if kind == "answer" {
                            if let Some(p) = pending.take() {
                                let answer = SdpAnswer::from_sdp_string(&sdp).map_err(|e| e.to_string())?;
                                rtc.sdp_api().accept_answer(p, answer).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    Ok(Cmd::Send(b)) => outq.push_back(b),
                    Ok(Cmd::Close) | Err(TryRecvError::Disconnected) => return Ok(()),
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Some(id) = cid {
                while let Some(b) = outq.front() {
                    let Some(mut ch) = rtc.channel(id) else { break };
                    if ch.buffered_amount() > 1 << 20 {
                        break;
                    }
                    ch.write(true, b).map_err(|e| e.to_string())?;
                    outq.pop_front();
                }
            } else if Instant::now() > deadline {
                return Err("collegamento diretto non riuscito".into());
            }
            let timeout = loop {
                match rtc.poll_output().map_err(|e| e.to_string())? {
                    Output::Timeout(t) => break t,
                    Output::Transmit(t) => {
                        let _ = sock.send_to(&t.contents, t.destination);
                    }
                    Output::Event(e) => match e {
                        Event::ChannelOpen(id, _) => {
                            cid = Some(id);
                            deliver(sess, peer.to_string(), DirectEvent::Open);
                        }
                        Event::ChannelData(d) => deliver(sess, peer.to_string(), DirectEvent::Data(d.data)),
                        Event::ChannelClose(_) => return Err("canale chiuso".into()),
                        Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                            return Err("collegamento interrotto".into())
                        }
                        _ => {}
                    },
                }
            };
            let wait = timeout.saturating_duration_since(Instant::now()).min(Duration::from_millis(25));
            if wait.is_zero() {
                rtc.handle_input(Input::Timeout(Instant::now())).map_err(|e| e.to_string())?;
                continue;
            }
            sock.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
            match sock.recv_from(&mut buf) {
                Ok((n, src)) => {
                    let dst = if src.ip().is_loopback() { locals.last().copied().unwrap_or(host) } else { host };
                    if let Ok(r) = Receive::new(Protocol::Udp, src, dst, &buf[..n]) {
                        rtc.handle_input(Input::Receive(Instant::now(), r)).map_err(|e| e.to_string())?;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    rtc.handle_input(Input::Timeout(Instant::now())).map_err(|e| e.to_string())?;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(module = "/web/rtc.js")]
    extern "C" {
        #[wasm_bindgen(js_name = rtcOpen)]
        fn rtc_open(id: u32, initiator: bool, stun: &str, cb: &Closure<dyn FnMut(u32, String, js_sys::Uint8Array)>);
        #[wasm_bindgen(js_name = rtcSignal)]
        fn rtc_signal(id: u32, kind: &str, sdp: &str);
        #[wasm_bindgen(js_name = rtcSend)]
        fn rtc_send(id: u32, bytes: &[u8]);
        #[wasm_bindgen(js_name = rtcClose)]
        fn rtc_close(id: u32);
    }

    thread_local! {
        static NEXT: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
    }

    pub struct Conn {
        id: u32,
    }

    impl Conn {
        pub fn open(sess: usize, peer: &str, initiator: bool) -> Conn {
            let id = NEXT.with(|n| {
                let v = n.get();
                n.set(v + 1);
                v
            });
            let peer = peer.to_string();
            // kind: 0 SDP da mandare (testo "tipo\nsdp"), 1 aperto, 2 dati, 3 chiuso (motivo)
            let cb = Closure::<dyn FnMut(u32, String, js_sys::Uint8Array)>::new(
                move |kind: u32, text: String, bytes: js_sys::Uint8Array| {
                    let ev = match kind {
                        0 => {
                            let (k, sdp) = text.split_once('\n').unwrap_or(("offer", &text));
                            DirectEvent::Signal(k.to_string(), sdp.to_string())
                        }
                        1 => DirectEvent::Open,
                        2 => DirectEvent::Data(bytes.to_vec()),
                        _ => DirectEvent::Closed(text),
                    };
                    deliver(sess, peer.clone(), ev);
                },
            );
            let stun: Vec<String> = STUN.iter().map(|s| format!("stun:{s}")).collect();
            rtc_open(id, initiator, &stun.join(","), &cb);
            cb.forget();
            Conn { id }
        }

        pub fn signal(&self, kind: &str, sdp: &str) {
            rtc_signal(self.id, kind, sdp);
        }

        pub fn send(&self, bytes: Vec<u8>) {
            rtc_send(self.id, &bytes);
        }

        pub fn close(&self) {
            rtc_close(self.id);
        }
    }
}

pub use imp::Conn;

#[cfg(test)]
mod tests {
    #[test]
    fn strips_hidden_candidates() {
        let sdp = "v=0\r\na=candidate:1 1 udp 2113937151 abcd-1234.local 54321 typ host\r\na=candidate:2 1 udp 1677729535 203.0.113.5 40000 typ srflx raddr 0.0.0.0 rport 0\r\n";
        let out = super::strip_mdns(sdp);
        assert!(!out.contains(".local"));
        assert!(out.contains("203.0.113.5"));
        assert!(out.ends_with("\r\n"));
    }
}
