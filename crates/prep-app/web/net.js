// WebSocket verso i relay della sincronizzazione tra dispositivi (vedi src/net.rs).
// Ci si ricollega da soli con attese crescenti finché wsClose non chiude il collegamento.
const links = new Map();

function allowedRelayUrl(url) {
  try {
    const u = new URL(url);
    return (
      u.protocol === "wss:" ||
      (u.protocol === "ws:" && ["localhost", "127.0.0.1", "::1", "[::1]"].includes(u.hostname))
    );
  } catch (e) {
    return false;
  }
}

export function wsOpen(id, url, cb) {
  if (!allowedRelayUrl(url)) {
    cb(2, "richiesto collegamento cifrato wss://");
    return;
  }
  const link = { url, cb, ws: null, wait: 2000, closed: false, queue: [] };
  links.set(id, link);
  connect(link);
}

function connect(link) {
  if (link.closed) return;
  let ws;
  try {
    ws = new WebSocket(link.url);
  } catch (e) {
    link.cb(2, String(e));
    retry(link);
    return;
  }
  link.ws = ws;
  ws.onopen = () => {
    link.wait = 2000;
    link.cb(0, "");
    for (const f of link.queue.splice(0)) ws.send(f);
  };
  ws.onmessage = (ev) => {
    if (typeof ev.data === "string") link.cb(1, ev.data);
  };
  ws.onclose = (ev) => {
    if (link.ws !== ws) return;
    link.ws = null;
    link.cb(2, ev.reason || "collegamento chiuso");
    retry(link);
  };
  ws.onerror = () => {};
}

function retry(link) {
  if (link.closed) return;
  setTimeout(() => connect(link), link.wait);
  link.wait = Math.min(link.wait * 2, 60000);
}

export function wsSend(id, text) {
  const link = links.get(id);
  if (!link) return;
  if (link.ws && link.ws.readyState === WebSocket.OPEN) link.ws.send(text);
  else if (link.queue.length < 200) link.queue.push(text);
}

export function wsClose(id) {
  const link = links.get(id);
  if (!link) return;
  link.closed = true;
  links.delete(id);
  if (link.ws) link.ws.close();
}
