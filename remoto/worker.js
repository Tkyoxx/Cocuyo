// el punto de cita: junta tu celular con tu pc y les pasa lo que se dicen. todo llega cifrado y aqui no hay ninguna llave

const SALA = /^\/sala\/([0-9a-f]{32})$/;
const MAXIMO = 2_000_000;
const aviso = (t) => JSON.stringify({ t });

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const sala = url.pathname.match(SALA);
    if (!sala) return new Response("Cocuyo: punto de cita. Aquí no hay nada que ver.", { status: 404 });
    const rol = url.searchParams.get("rol");
    if (rol !== "pc" && rol !== "cel") return new Response("Falta decir quién eres.", { status: 400 });
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return new Response("Solo WebSocket.", { status: 426 });
    return env.SALAS.get(env.SALAS.idFromName(sala[1])).fetch(request);
  },
};

export class Sala {
  constructor(ctx) {
    this.ctx = ctx;
    this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('{"t":"ping"}', '{"t":"pong"}'));
  }

  async fetch(request) {
    const rol = new URL(request.url).searchParams.get("rol");
    const otro = rol === "pc" ? "cel" : "pc";
    for (const viejo of this.ctx.getWebSockets(rol)) viejo.close(4000, "Entró otra conexión.");
    const [cliente, servidor] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(servidor, [rol]);
    for (const ws of this.ctx.getWebSockets(otro)) ws.send(aviso(rol === "pc" ? "pc-llego" : "cel-llego"));
    if (rol === "cel" && this.ctx.getWebSockets("pc").length === 0) servidor.send(aviso("sin-pc"));
    return new Response(null, { status: 101, webSocket: cliente });
  }

  webSocketMessage(ws, mensaje) {
    const largo = typeof mensaje === "string" ? mensaje.length : mensaje.byteLength;
    if (largo > MAXIMO) return ws.close(1009, "Demasiado grande.");
    const [rol] = this.ctx.getTags(ws);
    const otros = this.ctx.getWebSockets(rol === "pc" ? "cel" : "pc");
    if (!otros.length) {
      if (rol === "cel") ws.send(aviso("sin-pc"));
      return;
    }
    for (const otro of otros) otro.send(mensaje);
  }

  webSocketClose(ws) {
    const [rol] = this.ctx.getTags(ws);
    const otro = rol === "pc" ? "cel" : "pc";
    if (this.ctx.getWebSockets(rol).some((quedo) => quedo !== ws)) return;
    for (const o of this.ctx.getWebSockets(otro)) o.send(aviso(rol === "pc" ? "pc-se-fue" : "cel-se-fue"));
  }

  webSocketError(ws) {
    this.webSocketClose(ws);
  }
}
