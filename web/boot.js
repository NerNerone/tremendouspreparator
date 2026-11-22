import init from "./prepapp.js";

const loading = document.getElementById("loading");
init()
  .then(() => loading && loading.remove())
  .catch((e) => {
    if (loading) loading.textContent = "Errore di avvio: " + e;
  });

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("sw.js");
}
