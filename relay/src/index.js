/**
 * 하치봉고 '같이 하기' 중계. 둘의 하치봉고가 같은 방 코드로 붙으면, 한쪽이 보낸 줄을 다른 쪽에 그대로 넘긴다.
 *
 * - 넘기는 것은 어느 발로 쳤는지(왼쪽·오른쪽)와 박자, 마우스 위치·클릭뿐이다. 어떤 키였는지는 앱이 처음부터 보내지 않는다.
 * - 아무것도 저장하지 않고 기록하지 않는다(wrangler.toml 에서 로그도 끈다).
 * - 방 코드는 앱이 만든 난수다. 아는 사람만 들어오고, 한 방에는 둘까지만 들인다.
 *
 * Durable Object 하나가 방 하나다. 웹소켓은 잠재우기(hibernation)로 받아 아무도 안 칠 때는 깨어 있지 않는다.
 */

const ROOM = /^\/room\/([a-z2-9]{10})$/;
const ID = /^[a-z0-9]{8,32}$/;
const MEMBERS = 2;
/** 한 줄 상한. 앱이 보내는 줄은 100바이트 안팎이라, 넘으면 앱이 아닌 것이다. */
const MAX_LINE = 512;

export default {
  async fetch(req, env) {
    const m = ROOM.exec(new URL(req.url).pathname);
    if (!m) return new Response("hachibongo relay\n", { status: 404, headers: { "content-type": "text/plain; charset=utf-8" } });
    return env.ROOM.get(env.ROOM.idFromName(m[1])).fetch(req);
  },
};

const open = (ws) => ws.readyState === WebSocket.OPEN;

/** 보내기. 그새 닫힌 연결이면 조용히 버린다 — 한 연결의 마무리 때문에 방 전체가 예외로 멈추면 안 된다. */
function tell(ws, text) {
  try {
    ws.send(text);
  } catch {}
}

export class Room {
  constructor(ctx) {
    this.ctx = ctx;
    // 숨 쉬는 줄에는 깨우지 않고 바로 답한다.
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  idOf(ws) {
    return ws.deserializeAttachment()?.id;
  }

  async fetch(req) {
    if ((req.headers.get("Upgrade") ?? "").toLowerCase() !== "websocket") return new Response("websocket only", { status: 426 });
    const id = new URL(req.url).searchParams.get("id") ?? "";
    if (!ID.test(id)) return new Response("bad id", { status: 400 });

    // 같은 앱이 다시 붙으면(망이 바뀌었거나 반쯤 끊긴 연결) 앞 연결을 닫는다. 남이 세 번째로 들어오는 것만 막는다.
    const live = this.ctx.getWebSockets().filter(open);
    for (const ws of live) {
      if (this.idOf(ws) === id) {
        try {
          ws.close(4000, "replaced");
        } catch {}
      }
    }
    const others = new Set(live.map((ws) => this.idOf(ws)).filter((x) => x && x !== id));
    if (others.size >= MEMBERS) return new Response("room full", { status: 429 });

    const { 0: client, 1: server } = new WebSocketPair();
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ id });
    // 먼저 와 있던 쪽은 새로 온 쪽에게 자기 캐릭터를 다시 알려야 한다. 중계는 줄을 열어 보지 않으므로 알리라고만 한다.
    for (const ws of this.others(server)) tell(ws, '{"t":"join"}');
    tell(server, JSON.stringify({ t: "room", peers: others.size }));
    return new Response(null, { status: 101, webSocket: client });
  }

  others(me) {
    const id = this.idOf(me);
    return this.ctx.getWebSockets().filter((ws) => ws !== me && open(ws) && this.idOf(ws) !== id);
  }

  webSocketMessage(ws, msg) {
    if (typeof msg !== "string" || msg.length > MAX_LINE) return;
    for (const other of this.others(ws)) tell(other, msg);
  }

  webSocketClose(ws, code, reason) {
    // 같은 앱의 새 연결로 바뀐 것이면 상대에게 나갔다고 알리지 않는다.
    if (code !== 4000) {
      const id = this.idOf(ws);
      const still = this.ctx.getWebSockets().some((o) => o !== ws && open(o) && this.idOf(o) === id);
      if (!still) for (const other of this.others(ws)) tell(other, '{"t":"leave"}');
    }
    // 닫기를 마무리해야 목록에서 빠진다. 앱이 갑자기 꺼지면 1006 이 오는데 그 번호는 close() 에 줄 수 없다(1000·3000~4999 만).
    const ok = code === 1000 || (code >= 3000 && code <= 4999);
    try {
      ws.close(ok ? code : 1000, ok ? reason : "");
    } catch {}
  }

  webSocketError(ws) {
    this.webSocketClose(ws, 1011, "error");
  }
}
