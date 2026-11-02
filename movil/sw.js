// guarda la pagina en el celular para que abra sin red y no cambie a escondidas
const VERSION = "cocuyo-movil-dev";
const ARCHIVOS = ["index.html", "estilo.css", "app.js", "acciones.js", "informes.js", "enlace.js", "cifrado.js", "idioma.js", "icono.svg", "icono-180.png", "icono-192.png", "icono-512.png", "icono-maskable-512.png", "manifest.webmanifest"];

async function guardar() {
  const cache = await caches.open(VERSION);
  await Promise.all(
    ARCHIVOS.map(async (archivo) => {
      const respuesta = await fetch(archivo, { cache: "no-store" });
      if (!respuesta.ok) throw new Error(`${archivo}: ${respuesta.status}`);
      await cache.put(archivo, respuesta);
    }),
  );
}

self.addEventListener("install", (e) => e.waitUntil(guardar()));
self.addEventListener("activate", (e) => e.waitUntil(caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== VERSION).map((k) => caches.delete(k))))));
self.addEventListener("push", (e) => {
  let aviso = {};
  try {
    aviso = e.data?.json() ?? {};
  } catch {}
  e.waitUntil(self.registration.showNotification(aviso.titulo || "Cocuyo", { body: aviso.texto || "", icon: "icono-192.png", badge: "icono-192.png", tag: aviso.etiqueta || "cocuyo", data: { etiqueta: aviso.etiqueta || "" } }));
});
self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  const destino = new URL(e.notification.data?.etiqueta === "informe" ? "./?abrir=informes" : "./", self.registration.scope).href;
  e.waitUntil(
    clients.matchAll({ type: "window", includeUncontrolled: true }).then((abiertas) => {
      const ya = abiertas.find((c) => c.url.startsWith(self.registration.scope));
      return ya ? ya.focus() : clients.openWindow(destino);
    }),
  );
});
self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET" || new URL(e.request.url).origin !== location.origin) return;
  const pedido = e.request.mode === "navigate" ? "index.html" : e.request;
  e.respondWith(caches.open(VERSION).then((c) => c.match(pedido, { ignoreSearch: true })).then((hit) => hit ?? fetch(e.request)));
});
