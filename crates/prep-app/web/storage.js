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

// Dimensioni della vista del browser e notifica quando cambiano (rotazione, barra degli indirizzi).
export function viewportSize() {
  return [window.innerWidth, window.innerHeight];
}

export function onResize(cb) {
  window.addEventListener("resize", () => cb(window.innerWidth, window.innerHeight));
}
