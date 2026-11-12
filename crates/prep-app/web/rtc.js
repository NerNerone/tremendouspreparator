// Collegamento diretto (WebRTC DataChannel) con un dispositivo della stanza (vedi src/direct.rs).
// cb(kind, testo, byte): 0 = SDP da mandare ("tipo\nsdp"), 1 = aperto, 2 = dati, 3 = chiuso.
const conns = new Map();
const EMPTY = new Uint8Array(0);

function gathered(pc, ms) {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((res) => {
    const t = setTimeout(res, ms);
    pc.addEventListener("icegatheringstatechange", () => {
      if (pc.iceGatheringState === "complete") { clearTimeout(t); res(); }
    });
  });
}

function setup(c, dc) {
  c.dc = dc;
  dc.binaryType = "arraybuffer";
  dc.bufferedAmountLowThreshold = 256 * 1024;
  dc.onopen = () => { c.cb(1, "", EMPTY); flush(c); };
  dc.onmessage = (ev) => {
    if (ev.data instanceof ArrayBuffer) c.cb(2, "", new Uint8Array(ev.data));
  };
  dc.onbufferedamountlow = () => flush(c);
  dc.onclose = () => close(c, "canale chiuso");
}

function flush(c) {
  const dc = c.dc;
  while (dc && dc.readyState === "open" && c.queue.length && dc.bufferedAmount < 1024 * 1024) {
    dc.send(c.queue.shift());
  }
}

function close(c, why) {
  if (c.closed) return;
  c.closed = true;
  try { c.pc.close(); } catch (e) {}
  conns.delete(c.id);
  c.cb(3, why, EMPTY);
}

async function sendLocal(c, kind) {
  await gathered(c.pc, 3000);
  c.cb(0, kind + "\n" + c.pc.localDescription.sdp, EMPTY);
}

export function rtcOpen(id, initiator, stun, cb) {
  const iceServers = stun ? [{ urls: stun.split(",") }] : [];
  const pc = new RTCPeerConnection({ iceServers });
  const c = { id, pc, cb, dc: null, queue: [], closed: false };
  conns.set(id, c);
  pc.onconnectionstatechange = () => {
    if (pc.connectionState === "failed") close(c, "collegamento diretto non riuscito");
    if (pc.connectionState === "disconnected") close(c, "collegamento interrotto");
  };
  setTimeout(() => { if (!c.dc || c.dc.readyState !== "open") close(c, "collegamento diretto non riuscito"); }, 30000);
  if (initiator) {
    setup(c, pc.createDataChannel("tp"));
    pc.createOffer()
      .then((o) => pc.setLocalDescription(o))
      .then(() => sendLocal(c, "offer"))
      .catch((e) => close(c, String(e)));
  } else {
    pc.ondatachannel = (ev) => setup(c, ev.channel);
  }
}

export function rtcSignal(id, kind, sdp) {
  const c = conns.get(id);
  if (!c) return;
  const pc = c.pc;
  if (kind === "offer") {
    pc.setRemoteDescription({ type: "offer", sdp })
      .then(() => pc.createAnswer())
      .then((a) => pc.setLocalDescription(a))
      .then(() => sendLocal(c, "answer"))
      .catch((e) => close(c, String(e)));
  } else if (kind === "answer") {
    pc.setRemoteDescription({ type: "answer", sdp }).catch((e) => close(c, String(e)));
  }
}

export function rtcSend(id, bytes) {
  const c = conns.get(id);
  if (!c) return;
  c.queue.push(bytes.slice());
  flush(c);
}

export function rtcClose(id) {
  const c = conns.get(id);
  if (c) close(c, "");
}
