'use strict';

// 같이 하기 — 두 사람의 하치봉고를 중계(relay/)로 잇는다. 메인 창에서만 돈다(상대 창은 main.js 의 fromPeer 로 받기만 한다).
// 방 코드·트레이 메뉴·상대 창 띄우기는 Rust(together.rs)가 하고, 여기서는 중계와 줄을 주고받는다.
//
// 보내는 것은 어느 발로 쳤는지(L·R)와 박자, 마우스 위치(화면 안 비율)·클릭·휠, 내 캐릭터와 이름표 이름뿐이다. 어떤 키였는지는 보내지 않는다 —
// 보내면 상대 화면이 곧 내 키로거가 된다. 받는 쪽은 그 발 쪽 키 하나를 아무거나 골라 친다.
const together = (() => {
  const RELAY = 'wss://hachibongo-relay.ds187.workers.dev';
  // 마우스는 초당 15번까지만. 패드 위 마우스는 받는 쪽이 부드럽게 따라가고, 중계 요청 수도 무료 한도 안에 든다.
  const MOVE_MS = 66;
  const WHEEL_MS = 120;
  const PING_MS = 25_000;
  // 이만큼 아무것도 안 오면(숨 쉬는 줄의 답도) 반쯤 끊긴 연결로 보고 다시 붙는다
  const DEAD_MS = 70_000;
  const BACKOFF = [1000, 2000, 5000, 10_000, 30_000];

  let base = RELAY;
  let client = '';
  let room = null;
  let ws = null;
  let peer = null;
  let peerName = null;
  let retry = 0;
  let retryTimer = 0;
  let heardAt = 0;
  let seq = 0;
  const down = new Map();  // 내가 누르고 있는 키 -> 누름 번호. 떼는 줄을 누른 줄과 짝짓는 데만 쓴다.
  let moveTimer = 0;
  let moveAt = 0;
  let movePending = null;
  let moveSent = '';
  let wheelAt = 0;

  const status = (text) => T.core.invoke('together_status', { text }).catch(() => {});
  const nameOf = (c) => (Object.hasOwn(CHARS, c) ? CHARS[c].name : c);

  function send(obj) {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(typeof obj === 'string' ? obj : JSON.stringify(obj));
  }

  function hello() {
    send({ t: 'hello', c: document.documentElement.dataset.char || 'hachiware', n: tagName });
  }

  // 상대 창 띄우기·닫기는 Rust 에서 따로따로 돌아서, 닫기 바로 뒤의 띄우기가 먼저 끝나면 창이 사라진 채 남았다.
  // 번호를 붙여 Rust 가 가장 나중 것만 따르게 한다. 시각을 섞어 웹뷰를 새로 읽어도 번호가 뒤로 가지 않는다.
  let peerSeq = 0;
  const nextSeq = () => (peerSeq = Math.max(peerSeq + 1, Date.now() * 1000));

  function showPeer(c, tries = 0) {
    peer = c;
    const seq = nextSeq();
    T.core.invoke('peer', { character: c, seq }).catch(() => {
      // 창을 못 띄웠으면 몇 번 다시. 그새 상대가 바뀌었거나 나갔으면 그만둔다.
      if (tries < 3) setTimeout(() => { if (peer === c && peerSeq === seq) showPeer(c, tries + 1); }, 2000);
    });
  }

  function peerGone() {
    if (peer === null) return;
    peer = null;
    peerName = null;
    T.core.invoke('peer', { character: null, seq: nextSeq() }).catch(() => {});
  }

  function connect() {
    clearTimeout(retryTimer);
    if (!room || !client) return;
    status('연결하는 중');
    const sock = new WebSocket(`${base}/room/${room}?id=${client}`);
    ws = sock;
    sock.onopen = () => {
      if (ws !== sock) return;
      retry = 0;
      heardAt = Date.now();
      hello();
    };
    sock.onmessage = (ev) => {
      if (ws !== sock) return;
      heardAt = Date.now();
      if (ev.data === 'pong') return;
      let m;
      try {
        m = JSON.parse(ev.data);
      } catch {
        return;
      }
      onLine(m);
    };
    sock.onclose = () => lost(sock);
  }

  /** 연결을 놓고 잠시 뒤 다시 붙는다. 반쯤 끊긴 연결은 close() 를 불러도 닫힘 신호가 한참 뒤에 와서 기다리지 않는다. */
  function lost(sock) {
    if (ws !== sock) return;
    ws = null;
    down.clear();
    peerGone();
    if (!room) return;
    status('끊겼어요 · 다시 잇는 중');
    retryTimer = setTimeout(connect, BACKOFF[Math.min(retry++, BACKOFF.length - 1)]);
  }

  function onLine(m) {
    if (!m || typeof m !== 'object') return;
    switch (m.t) {
      case 'room':
        if (!m.peers) status('혼자 있어요 · 상대를 기다리는 중');
        break;
      case 'join':
        // 새로 온 상대는 내 캐릭터를 모른다
        hello();
        break;
      case 'leave':
        peerGone();
        status('상대가 나갔어요 · 기다리는 중');
        break;
      case 'hello': {
        if (typeof m.c !== 'string' || !Object.hasOwn(CHARS, m.c)) return;
        const first = peer === null;
        if (peer !== m.c) showPeer(m.c);
        // 처음 만난 상대에게는 내 캐릭터도 다시 알린다. 들어왔다는 알림(join)에 답한 인사가 길에서 사라지면
        // 상대는 내 캐릭터를 모른 채 다시 접속할 때까지 빈손으로 남는다.
        if (first) hello();
        // 이름은 인사마다 실려 온다(상대가 이름을 바꾸면 인사를 다시 보낸다). 옛 판은 이름 없이 온다.
        const n = cleanName(m.n);
        if (n !== peerName) {
          peerName = n;
          T.core.invoke('peer_name', { name: n || null }).catch(() => {});
        }
        status(`같이 하는 중 · 상대는 ${n ? `${n}(${nameOf(m.c)})` : nameOf(m.c)}`);
        break;
      }
      default:
        if (peer !== null) T.event.emitTo('peer', 'peer', m).catch(() => {});
    }
  }

  function queueMove(nx, ny) {
    movePending = [Math.round(nx * 1000) / 1000, Math.round(ny * 1000) / 1000];
    if (moveTimer) return;
    moveTimer = setTimeout(() => {
      moveTimer = 0;
      moveAt = performance.now();
      const [x, y] = movePending;
      // 창을 옮길 때도 커서 줄이 오는데, 커서는 그대로라 보낼 것이 없다
      if (`${x},${y}` === moveSent) return;
      moveSent = `${x},${y}`;
      send({ t: 'm', x, y });
    }, Math.max(0, MOVE_MS - (performance.now() - moveAt)));
  }

  /** 내 입력 하나를 상대에게. 줄은 main.js 가 그 입력을 그린 뒤에 넘긴다. */
  function share(e) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    switch (e.t) {
      case 'k':
        if (e.d) {
          if (down.has(e.vk)) return;
          const key = keyByVk.get(e.vk) ?? spaceKey;
          const i = ++seq;
          down.set(e.vk, i);
          send({ t: 'k', s: key.side, d: 1, i });
        } else {
          const i = down.get(e.vk);
          if (i === undefined) return;
          down.delete(e.vk);
          send({ t: 'k', d: 0, i });
        }
        break;
      case 'b':
        if (e.b >= 0 && e.b <= 2) send({ t: 'b', b: e.b, d: e.d ? 1 : 0 });
        break;
      case 'w': {
        const t = performance.now();
        if (t - wheelAt > WHEEL_MS) {
          wheelAt = t;
          send({ t: 'w' });
        }
        break;
      }
      case 'm':
        queueMove(e.nx, e.ny);
        break;
    }
  }

  /** 방을 바꾼다. null 이면 나간다. Rust 가 트레이 메뉴를 누를 때 알린다. */
  function join(code) {
    if (code === room) return;
    room = code;
    const old = ws;
    ws = null;
    down.clear();
    if (old) {
      try {
        old.close(1000);
      } catch {}
    }
    peerGone();
    clearTimeout(retryTimer);
    retry = 0;
    if (room) connect();
  }

  function start(s) {
    base = s.relay || RELAY;
    client = s.client || '';
    setInterval(() => {
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      if (Date.now() - heardAt > DEAD_MS) {
        const sock = ws;
        lost(sock);
        try {
          sock.close();
        } catch {}
        return;
      }
      send('ping');
    }, PING_MS);
    join(s.room ?? null);
  }

  return { start, join, share, hello };
})();
