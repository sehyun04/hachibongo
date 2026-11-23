// 중계 검증. `node relay/test.mjs [중계 주소]` — 주소를 안 주면 이 PC 의 wrangler dev(ws://127.0.0.1:8787).
// 실제 중계에 돌릴 때는 방 코드를 매번 새로 만들어 남의 방에 닿지 않는다.
const base = process.argv[2] ?? 'ws://127.0.0.1:8787';
const abc = 'abcdefghjkmnpqrstuvwxyz23456789';
const pick = (n) => Array.from({ length: n }, () => abc[Math.floor(Math.random() * abc.length)]).join('');
const room = pick(9) + '7';

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
function client(id) {
  const ws = new WebSocket(`${base}/room/${room}?id=${id}`);
  const got = [];
  ws.onmessage = (e) => got.push(e.data);
  const opened = new Promise((ok, no) => {
    ws.onopen = ok;
    ws.onerror = () => no(new Error('열리지 않음'));
  });
  return { ws, got, opened, closed: new Promise((r) => (ws.onclose = r)) };
}

let failed = 0;
function check(name, ok, detail = '') {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + detail}`);
  if (!ok) failed++;
}

const a = client('aaaaaaaa1');
await a.opened;
await wait(300);
check('혼자 들어오면 상대 0', a.got.includes('{"t":"room","peers":0}'), JSON.stringify(a.got));

const b = client('bbbbbbbb2');
await b.opened;
await wait(300);
check('먼저 온 쪽에 들어왔다고 알림', a.got.includes('{"t":"join"}'), JSON.stringify(a.got));
check('나중 온 쪽은 상대 1', b.got.includes('{"t":"room","peers":1}'), JSON.stringify(b.got));

a.ws.send('{"t":"hello","c":"usagi"}');
b.ws.send('{"t":"k","s":"L","d":1,"i":1}');
await wait(300);
check('줄을 상대에게 그대로', b.got.includes('{"t":"hello","c":"usagi"}'), JSON.stringify(b.got));
check('반대쪽도', a.got.includes('{"t":"k","s":"L","d":1,"i":1}'), JSON.stringify(a.got));
check('제 줄은 저에게 안 돌아옴', !a.got.includes('{"t":"hello","c":"usagi"}'));

a.ws.send('x'.repeat(600));
await wait(300);
check('긴 줄은 버림', !b.got.some((g) => g.length > 512));

a.ws.send('ping');
await wait(300);
check('숨 쉬는 줄에 답', a.got.includes('pong'), JSON.stringify(a.got));

const c = client('cccccccc3');
let thirdIn = true;
try {
  await c.opened;
} catch {
  thirdIn = false;
}
check('세 번째 사람은 못 들어옴', !thirdIn);

// 같은 앱이 다시 붙으면 앞 연결만 닫히고 상대에게 나갔다고 하지 않는다
const before = b.got.length;
const a2 = client('aaaaaaaa1');
await a2.opened;
await a.closed;
await wait(400);
check('같은 앱 재접속: 앞 연결 닫힘', a.ws.readyState === WebSocket.CLOSED);
check('같은 앱 재접속: 나갔다고 안 함', !b.got.slice(before).includes('{"t":"leave"}'), JSON.stringify(b.got.slice(before)));
a2.ws.send('{"t":"w"}');
await wait(300);
check('다시 붙은 연결로도 넘어감', b.got.includes('{"t":"w"}'));

a2.ws.close(1000);
await wait(500);
check('나가면 상대에게 알림', b.got.includes('{"t":"leave"}'), JSON.stringify(b.got));
b.ws.close(1000);
await wait(200);

console.log(failed ? `${failed} 실패` : '전부 통과');
process.exit(failed ? 1 : 0);
