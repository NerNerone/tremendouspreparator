// Ponte tra l'app (WebAssembly) e il browser: archivio in IndexedDB e scelta di un file.
const DB_NAME = "tremendouspreparator";
const STORE = "files";

let dbPromise = null;
function db() {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }
  return dbPromise;
}

// Chiede al browser di non svuotare da solo l'archivio dell'app (contenuti importati e
// progressi) quando lo spazio scarseggia. Il browser può concederlo o meno.
async function requestPersistence() {
  try {
    if (navigator.storage && navigator.storage.persist && !(await navigator.storage.persisted())) {
      await navigator.storage.persist();
    }
  } catch (_) {
    // non supportato: nessun problema, i dati restano comunque salvati
  }
}

// Tutti i file salvati, come array di [percorso, Uint8Array].
export async function loadAll() {
  requestPersistence();
  const d = await db();
  return new Promise((resolve, reject) => {
    const out = [];
    const req = d.transaction(STORE, "readonly").objectStore(STORE).openCursor();
    req.onsuccess = () => {
      const c = req.result;
      if (c) {
        out.push([c.key, new Uint8Array(c.value)]);
        c.continue();
      } else {
        resolve(out);
      }
    };
    req.onerror = () => reject(req.error);
  });
}

export function putFile(path, bytes) {
  // copia: la memoria di WebAssembly può essere riutilizzata prima che la scrittura finisca
  const copy = bytes.slice();
  db().then((d) => d.transaction(STORE, "readwrite").objectStore(STORE).put(copy, path));
}

export function deleteFile(path) {
  db().then((d) => d.transaction(STORE, "readwrite").objectStore(STORE).delete(path));
}

// Apre il selettore di file; restituisce i byte del file scelto (o null).
export function pickFile() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".gz,.json,application/gzip,application/json";
    input.onchange = async () => {
      const f = input.files && input.files[0];
      resolve(f ? new Uint8Array(await f.arrayBuffer()) : null);
    };
    input.click();
  });
}

// Selettore di un file qualsiasi per la pagina «Contenuti»: [nome, byte] o null.
export function pickNamedFile() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,.gz,.md,.markdown,.txt,.svg";
    input.onchange = async () => {
      const f = input.files && input.files[0];
      resolve(f ? [f.name, new Uint8Array(await f.arrayBuffer())] : null);
    };
    input.click();
  });
}

// Scarica un file generato dall'app (pacchetto esportato).
export function downloadFile(name, bytes) {
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/gzip" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}

// Selettore di immagini (foto profilo o del gruppo).
export function pickImage() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = async () => {
      const f = input.files && input.files[0];
      resolve(f ? new Uint8Array(await f.arrayBuffer()) : null);
    };
    input.click();
  });
}

// Notifica del browser (se permessa); altrimenti resta l'avviso nell'app.
export function notifyNow(text) {
  try {
    const [title, body] = text.split("\n");
    if (!("Notification" in window)) return;
    if (Notification.permission === "granted") new Notification(title, { body: body || "" });
    else if (Notification.permission !== "denied") Notification.requestPermission();
  } catch (e) {}
}

// Dimensioni della vista del browser e notifica quando cambiano (rotazione, barra degli indirizzi).
export function viewportSize() {
  return [window.innerWidth, window.innerHeight];
}

export function onResize(cb) {
  window.addEventListener("resize", () => cb(window.innerWidth, window.innerHeight));
}

// Condivisione di un testo (inviti al gruppo): menu del sistema se c'è, altrimenti appunti.
export function openUrl(url) {
  try {
    const u = new URL(url);
    if (u.protocol !== "https:" && u.protocol !== "http:") return false;
  } catch (e) {
    return false;
  }
  return window.open(url, "_blank", "noopener,noreferrer") !== null;
}

export function shareText(text) {
  if (navigator.share) {
    navigator.share({ text }).catch(() => {});
    return true;
  }
  if (navigator.clipboard) {
    navigator.clipboard.writeText(text).catch(() => {});
  }
  return false;
}

// Parte dell'indirizzo dopo «#» (inviti), tolta subito dall'indirizzo.
export function takeHash() {
  const h = location.hash || "";
  if (h) history.replaceState(null, "", location.pathname + location.search);
  return h;
}
