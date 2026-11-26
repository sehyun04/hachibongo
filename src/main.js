'use strict';

const T = window.__TAURI__;
const NS = 'http://www.w3.org/2000/svg';
const svg = document.getElementById('stage');
const $ = (id) => document.getElementById(id);
// 같이 하기의 상대 창이면 Rust 가 창을 만들 때 심어 둔다(together.rs). 상대 창은 내 입력이 아니라 상대 입력으로 움직인다.
const HACHI = window.__HACHI ?? {};
const PEER = HACHI.role === 'peer';
// 이벤트는 내 창 앞으로 온 것만 받는다. 그냥 listen 하면 Rust 가 다른 창 앞으로 보낸 것까지 받아서,
// 상대 캐릭터가 바뀔 때 내 캐릭터도 같이 바뀌었다.
const on = (name, fn) => T.event.listen(name, fn, { target: PEER ? 'peer' : 'main' });
// 지금 캐릭터(chars.js). 시작할 때 setChar 가 정한다.
let C = CHARS.hachiware;

// ---------------------------------------------------------------- 키보드

// 60% 배열에 방향키만 오른쪽 아래로 끼운 15칸 폭. 화면에 없는 키(F열, 숫자패드,
// Home/End 등)는 손이 가는 자리가 비슷한 키에 붙여서 발이 엉뚱한 데를 치지 않게 했다.
const ROWS = [
  [[1, 0xc0, 0x1b], [1, 0x31, 0x70, 0x61], [1, 0x32, 0x71, 0x62], [1, 0x33, 0x72, 0x63], [1, 0x34, 0x73, 0x64],
   [1, 0x35, 0x74, 0x65], [1, 0x36, 0x75, 0x66], [1, 0x37, 0x76, 0x67], [1, 0x38, 0x77, 0x68, 0x6a],
   [1, 0x39, 0x78, 0x69], [1, 0x30, 0x79, 0x60], [1, 0xbd, 0x7a, 0x6d], [1, 0xbb, 0x7b, 0x6b],
   [2, 0x08, 0x2e, 0x2c, 0x13, 0x91]],
  [[1.5, 0x09], [1, 0x51], [1, 0x57], [1, 0x45], [1, 0x52], [1, 0x54], [1, 0x59], [1, 0x55], [1, 0x49],
   [1, 0x4f], [1, 0x50], [1, 0xdb, 0x21], [1, 0xdd, 0x22], [1.5, 0xdc, 0x2d, 0x24]],
  [[1.75, 0x14, 0x90], [1, 0x41], [1, 0x53], [1, 0x44], [1, 0x46], [1, 0x47], [1, 0x48], [1, 0x4a], [1, 0x4b],
   [1, 0x4c], [1, 0xba], [1, 0xde, 0x23], [2.25, 0x0d]],
  [[2.25, 0xa0, 0x10], [1, 0x5a], [1, 0x58], [1, 0x43], [1, 0x56], [1, 0x42], [1, 0x4e], [1, 0x4d],
   [1, 0xbc, 0x6e], [1, 0xbe], [1, 0xbf, 0x6f], [1.75, 0xa1], [1, 0x26]],
  // 한/영(0x15)은 오른쪽 Alt 자리, 한자(0x19)는 오른쪽 Ctrl 자리
  [[1.25, 0xa2, 0x11], [1.25, 0x5b], [1.25, 0xa4, 0x12], [6.25, 0x20], [1, 0xa5, 0x15],
   [1, 0xa3, 0x19, 0x5c, 0x5d], [1, 0x25], [1, 0x28], [1, 0x27]],
];
const KB = { x: 90, y: 195, w: 180, h: 45, gap: 1.3 };
const KEY_SIDE = 2.4;   // 키캡 옆면이 보이는 높이
const KEY_TRAVEL = 1.2; // 눌렀을 때 윗면이 내려가는 거리(style.css 와 같게)
const KB_MID = KB.x + KB.w / 2;
// 이 선 왼쪽 키는 왼발, 오른쪽 키는 오른발. 키보드 가운데가 아니라 하치 몸 가운데라서
// 오른발은 키 몇 개만 맡고 대부분 마우스에 있는다.
const SPLIT_X = 200;

// ---------------------------------------------------------------- 원근

// 책상·키보드·패드는 모두 한 바닥 위에 있다고 보고 같은 소실점(책상 가운데)으로 좁힌다.
// 판마다 제 가운데로 좁히면 물건끼리 따로 논다. 같은 깊이 간격도 뒤쪽이 화면에서 더 짧다.
// SVG 에는 원근 변환이 없어서 점마다 직접 옮긴다. 하치가 정면 평면 그림이라 back 을 이보다 낮추면 따로 논다.
const FLOOR = { cx: 222, y0: 185.5, y1: 262, back: 0.84 };

// 평면 좌표(앞 끝 기준 크기)를 화면 좌표로. 셋째 값은 그 깊이에서 줄어든 비율.
function project(x, y) {
  const b = FLOOR.back;
  const u = Math.min(1, Math.max(0, (y - FLOOR.y0) / (FLOOR.y1 - FLOOR.y0)));
  const s = 1 / (1 / b - u * (1 / b - 1));
  const t = (s - b) / (1 - b);
  return [FLOOR.cx + (x - FLOOR.cx) * s, FLOOR.y0 + t * (FLOOR.y1 - FLOOR.y0), s];
}

// 화면 y 에서의 줄어든 비율. 평면 위를 움직이는 마우스 크기에 쓴다.
const floorScale = (sy) =>
  FLOOR.back + (1 - FLOOR.back) * Math.min(1, Math.max(0, (sy - FLOOR.y0) / (FLOOR.y1 - FLOOR.y0)));

function quad(x, y, w, h) {
  return [project(x, y), project(x + w, y), project(x + w, y + h), project(x, y + h)];
}

// 네 꼭짓점 사각형의 모서리를 둥글린 경로. r 은 모서리마다 따로 줄 수 있다.
function roundedPath(pts, r) {
  const n = pts.length;
  const rad = (i) => (Array.isArray(r) ? r[i % n] : r);
  const toward = (a, b, d) => {
    const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
    const k = Math.min(d, len / 2) / len;
    return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
  };
  const f = (p) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`;
  let d = `M ${f(toward(pts[0], pts[1], rad(0)))}`;
  for (let i = 1; i <= n; i++) {
    const p = pts[i % n];
    d += ` L ${f(toward(p, pts[i - 1], rad(i)))} Q ${f(p)} ${f(toward(p, pts[(i + 1) % n], rad(i)))}`;
  }
  return d + ' Z';
}

const shiftPts = (pts, dx, dy) => pts.map(([x, y]) => [x + dx, y + dy]);

// 판을 그 두께만큼 아래로 민 것이 앞면, 다시 빛 반대쪽(오른쪽 아래)으로 민 것이 그림자
function slab(top, name, r, thick, sx, sy) {
  const bottom = shiftPts(top, 0, thick);
  $(name).setAttribute('d', roundedPath(top, r));
  $(name + 'Front').setAttribute('d', roundedPath(bottom, r));
  $(name + 'Shadow').setAttribute('d', roundedPath(shiftPts(bottom, sx, sy), r + 2));
}

{
  const back = [project(76, FLOOR.y0), project(368, FLOOR.y0)];
  const top = [back[0], back[1], [368, FLOOR.y1], [76, FLOOR.y1]];
  const deskTop = roundedPath(top, [8, 8, 0, 0]);
  const deskFront = 'M 76 262 H 368 V 264 Q 368 272 360 272 H 84 Q 76 272 76 264 Z';
  $('deskTop').setAttribute('d', deskTop);
  $('deskFront').setAttribute('d', deskFront);
  $('deskLine').setAttribute('d', `${deskTop} ${deskFront}`);
  slab(quad(86, 191, 188, 53), 'kb', 6, 5, 3.5, 4);
  slab(quad(282, 189, 82, 60), 'pad', 9, 3, 3, 3.5);
}

const keyByVk = new Map();
let spaceKey = null;
// 상대가 친 발 쪽에서 고를 키. 상대가 어떤 키를 쳤는지는 오지 않아서(together.js) 글자·숫자 줄의 보통 키 중 아무거나 친다.
const PEER_POOL = { L: [], R: [] };
{
  const u = KB.w / 15;
  const rh = KB.h / ROWS.length;
  ROWS.forEach((row, r) => {
    let off = 0;
    for (const [width, ...vks] of row) {
      const x = KB.x + off * u + KB.gap / 2;
      const y = KB.y + r * rh + KB.gap / 2;
      const w = width * u - KB.gap;
      const h = rh - KB.gap;
      const el = document.createElementNS(NS, 'g');
      el.setAttribute('class', 'key');
      const side = document.createElementNS(NS, 'path');
      setAttrs(side, { d: roundedPath(quad(x, y, w, h), 1.8), class: 'key-side' });
      const top = document.createElementNS(NS, 'path');
      setAttrs(top, { d: roundedPath(quad(x + 0.5, y, w - 1, h - KEY_SIDE), 1.6), class: 'key-top' });
      el.append(side, top);
      $('keys').appendChild(el);
      const [lx0] = project(x, y + h / 2);
      const [lx1] = project(x + w, y + h / 2);
      const cx = (lx0 + lx1) / 2;
      // cy 는 윗면 가운데. 발은 윗면을 친다.
      const cy = project(x + w / 2, y + (h - KEY_SIDE) / 2)[1];
      const key = { el, side: cx < SPLIT_X ? 'L' : 'R', x0: lx0, x1: lx1, cx, cy, held: 0, vk: vks[0] };
      if (r < 4 && width === 1) PEER_POOL[key.side].push(key);
      for (const vk of vks) keyByVk.set(vk, key);
      if (vks.includes(0x20)) spaceKey = key;
      off += width;
    }
  });
}

// ---------------------------------------------------------------- 상태

// 팔은 그림 속 원래 팔이 붙은 옆구리에서 나온다. 턱 밑 가운데에서 빼면 머리에 팔이 달린 것처럼 보였다.
// 안쪽 키를 칠 때는 뿌리가 아랫몸 바깥 1/3 안에서만 안쪽으로 따라 들어와 팔이 길어지지 않게 한다.
const SHOULDER = { L: { x: 118, y: 182 }, R: { x: 283, y: 182 } };
const ROOT_SLIDE = { L: [118, 160], R: [240, 283] };
const PRESS_LIFT = 13.4 - KEY_TRAVEL;  // 발 아래 끝이 윗면 가운데쯤. 키가 내려간 만큼 발도 같이 내려간다
const HOVER_LIFT = 18;
const MOUSE_RANGE = { x0: 296, x1: 344, y0: 206, y1: 230 };  // 패드 평면 좌표
// 발은 마우스 등(위쪽)을 쥔다. 아래쪽 버튼이 보여야 클릭이 보인다.
const MOUSE_GRIP = 12;
const MIN_HOLD = 70;      // 아주 짧은 타건도 한 번은 눌린 모습이 보이게
const HOVER_MS = 650;     // 손을 뗀 뒤 그 키 위에 머무는 시간
const SLEEP_MS = 60_000;
const BURST_ON = 3000;    // 이만큼 이어 쳐야 얼굴이 바뀐다. 몇 자 칠 때마다 바뀌면 정신없다
const BURST_GAP = 1500;   // 키 사이가 이보다 비면 끊긴 것으로 본다. 단어 사이에 쉬는 정도로는 안 끊긴다

const held = [];          // 지금 눌린 키, 마지막에 누른 것이 끝
const lastUp = { L: { key: null, at: -1e9 }, R: { key: null, at: -1e9 } };
const buttons = [false, false, false];
const mouseTarget = { x: 315, y: 217 };
const mouse = { x: 315, y: 217 };
const now = () => performance.now();
const restL = () => {
  const f = keyByVk.get(0x46);
  return { x: f.cx, y: f.cy - HOVER_LIFT };
};
// 마우스는 패드 뒤로 갈수록 작아지므로 쥐는 자리도 같이 줄인다
const gripY = () => mouse.y - MOUSE_GRIP * floorScale(mouse.y);
const paw = { L: { ...restL(), lift: 8 }, R: { x: mouse.x, y: gripY(), lift: 1 } };
// 발이 판에서 떠 있는 높이. 그림자 크기·진하기를 정한다.
const LIFT = { press: 0, hover: 7, rest: 8, mouse: 1 };

// 몸 기울기. 치는 동안은 치는 키 쪽, 마우스를 움직이면 화면 속 커서 쪽을 본다.
const TILT_MAX = 4;       // 도. 이보다 크면 귀 끝이 10 넘게 흔들려 정신없다
const PIVOT = { x: 200, y: 196 };
const body = { tilt: 0, nod: 0, lean: 0 };
// 몸이 치는 키·마우스 쪽으로 살짝 쏠린다. 키보드·마우스 범위를 넓혀도 팔이 다 떠맡지 않게 하려는 것.
const LEAN = { min: -8, max: 20 };
let lastKey = null;
let lastKeyAt = -1e9;
let lastMoveAt = -1e9;
let lastMouseAct = -1e9;  // 오른발이 마우스로 돌아갈 때를 가른다
let cursorNx = 0.5;

// 원작 꼬리: 통째로 귀와 같은 파랑, 팔만큼 굵고 짧으며 곧게 서서 끝이 둥글게 한쪽으로 휜다(나가노 뒷모습 그림).
// 화면엔 머리만 보이고 몸은 책상에 가려 있으니 꼬리도 책상 뒤에서 올라온다. 옆구리에서 빼면 머리에 붙어 보였다.
// 왼쪽은 책상 모서리와 몸 사이가 좁아 다시 머리에 붙으므로 책상 뒤가 넓게 빈 오른쪽에 둔다.
// 뿌리는 책상 윗면 아래에 숨긴다. 마디 각도는 화면 기준 도(-90 이 위, 0 이 오른쪽).
// 머리 오른쪽 아랫선(약 -54도, 위로 갈수록 서는 곡선)보다 눕혀서 책상 선에서 둘이 만나 위로 벌어지는 V 가 되게 한다.
// 끝 마디만 살짝 세워 끝이 들린다.
const TAIL_BASE = { x: 296, y: 196 };
const TAIL_SEG = 14;
const TAIL_ANGLES = [-40, -42, -46, -54];
const TAIL_SLEEP = 14;    // 졸 때는 바깥으로 축 쓰러진다
const TAIL_MAX = 14;      // 이보다 크게 흔들면 머리에 닿거나 창 오른쪽 끝에 잘린다
// 클릭이나 가끔의 휘두르기는 뿌리 용수철로 한 번 튕긴다.
const TAIL_W = 13;        // rad/s, 한 번 튕기면 두 번쯤 흔들리고 멎는다
const TAIL_DAMP = 0.3;
const TAIL_LAG = 40;      // ms, 끝 마디가 늦게 따라와 물결처럼 휜다
// 치는 동안의 살랑거림은 꼬리가 몸 뒤에서 세로축으로 좌우로 쓸리는 것으로 본다. 뿌리에서 가로로 떨어진 만큼
// 비례해 가로로만 움직인다. 뿌리 축으로 돌리면 대각선 꼬리 끝이 위아래로 까딱여 좌우로 보이지 않았다.
const SWAY = 0.26;        // 가로 폭 배율
// 안쪽으로 서는 쪽은 머리 옆선이 가까워 흔들림 가운데를 바깥으로 민다. 0 이면 서는 끝에서 머리와 3 만큼만 떨어졌다.
const SWAY_OUT = 0.08;
const SWAY_WAVE = 0.45;   // rad, 마디마다 늦게 따라와 끝이 휘청이며 따라온다
const SWAY_MS = 1000;     // 한 번 왕복. 키 치는 박자보다 느려야 살랑거린다
const SWAY_HOLD = 1200;   // 마지막 키 뒤 이만큼은 계속 흔든다
const tail = { w: TAIL_ANGLES.map(() => 0), v: 0, dir: 1, env: 0, phase: 0 };

function tapLines(cx, cy, spread) {
  const p = document.createElementNS(NS, 'path');
  const s = spread;
  p.setAttribute('d',
    `M ${cx - s} ${cy - 1} l -4 -2 M ${cx - s + 1} ${cy - 6} l -3 -3 ` +
    `M ${cx + s} ${cy - 1} l 4 -2 M ${cx + s - 1} ${cy - 6} l 3 -3`);
  p.setAttribute('class', 'tap');
  $('fx').appendChild(p);
  setTimeout(() => p.remove(), 280);
}

function keyDown(vk) {
  if (held.some((h) => h.vk === vk)) return;
  const key = keyByVk.get(vk) ?? spaceKey;
  held.push({ vk, key, at: now() });
  key.held++;
  key.el.classList.add('lit');
  tapLines(hitX(key.side, key), key.cy - 3, 14);
  const t = now();
  if (t - lastKeyAt > BURST_GAP || lastMouseAct > lastKeyAt) streakFrom = t;
  lastKey = key;
  lastKeyAt = t;
  body.nod = 1.6;
  burst(t - streakFrom >= BURST_ON);
  clearTimeout(burstTimer);
  burstTimer = setTimeout(() => burst(false), BURST_GAP);
}

function flick(speed) {
  tail.v += speed * tail.dir;
  tail.dir = -tail.dir;
}

function keyUp(vk) {
  const entry = held.find((h) => h.vk === vk);
  if (!entry || entry.releasing) return;
  entry.releasing = true;
  const release = () => {
    const i = held.indexOf(entry);
    if (i >= 0) held.splice(i, 1);
    if (--entry.key.held <= 0) {
      entry.key.held = 0;
      entry.key.el.classList.remove('lit');
    }
    lastUp[entry.key.side] = { key: entry.key, at: now() };
    kick();
  };
  const wait = MIN_HOLD - (now() - entry.at);
  if (wait > 0) setTimeout(release, wait); else release();
}

function button(b, down) {
  if (b > 2) return;
  buttons[b] = down;
  lastMouseAct = now();
  burst(false);
  const el = b === 2 ? $('wheel') : $('btn' + b);
  el.classList.toggle('lit', down);
  if (down) {
    tapLines(mouse.x, mouse.y + 5, 14);
    body.nod = 1;
    flick(70);
  }
}

let wheelTimer = 0;
function wheel() {
  lastMouseAct = now();
  burst(false);
  $('wheel').classList.add('lit');
  clearTimeout(wheelTimer);
  wheelTimer = setTimeout(() => { if (!buttons[2]) $('wheel').classList.remove('lit'); }, 140);
}

// ---------------------------------------------------------------- 입력

let ignoring = true;
let locked = false;
let win = null;

function handle(e) {
  switch (e.t) {
    case 'k':
      e.d ? keyDown(e.vk) : keyUp(e.vk);
      active();
      break;
    case 'b':
      button(e.b, e.d);
      active();
      break;
    case 'w':
      wheel();
      active();
      break;
    case 'm':
      // 패드도 뒤가 좁은 사다리꼴이라 마우스 좌우 폭도 뒤쪽에서 같이 좁아진다
      [mouseTarget.x, mouseTarget.y] = project(
        MOUSE_RANGE.x0 + clamp01(e.nx) * (MOUSE_RANGE.x1 - MOUSE_RANGE.x0),
        MOUSE_RANGE.y0 + clamp01(e.ny) * (MOUSE_RANGE.y1 - MOUSE_RANGE.y0));
      if (Math.abs(e.nx - cursorNx) > 1e-4) {
        lastMoveAt = lastMouseAct = now();
        burst(false);
      }
      cursorNx = e.nx;
      // 상대 마우스는 내 창 어디에 커서가 있는지와 상관없다
      if (e.lx !== undefined) hitTest(e.lx, e.ly);
      active();
      break;
    case 'h':
      // 상대 창에 들어오는 내 커서. 끌기·클릭 통과만 가르고 캐릭터는 깨우지 않는다.
      hitTest(e.lx, e.ly);
      break;
  }
  kick();
}

// 상대가 보낸 줄(together.js 가 중계에서 받아 넘긴다). 중계를 거쳐 온 것이라 모양을 다시 본다.
const peerKeys = new Map();  // 상대의 누름 번호 -> 이쪽에서 고른 키
function fromPeer(m) {
  if (!m || typeof m !== 'object') return;
  switch (m.t) {
    case 'k':
      if (m.d) {
        if (peerKeys.has(m.i)) return;
        const pool = PEER_POOL[m.s === 'R' ? 'R' : 'L'];
        let key = pool[(Math.random() * pool.length) | 0];
        for (let n = 0; n < 4 && key.held; n++) key = pool[(Math.random() * pool.length) | 0];
        peerKeys.set(m.i, { vk: key.vk, at: now() });
        handle({ t: 'k', vk: key.vk, d: true });
      } else {
        const p = peerKeys.get(m.i);
        if (!p) return;
        peerKeys.delete(m.i);
        handle({ t: 'k', vk: p.vk, d: false });
      }
      break;
    case 'b':
      if (m.b === 0 || m.b === 1 || m.b === 2) handle({ t: 'b', b: m.b, d: !!m.d });
      break;
    case 'w':
      handle({ t: 'w' });
      break;
    case 'm':
      if (Number.isFinite(m.x) && Number.isFinite(m.y)) handle({ t: 'm', nx: clamp01(m.x), ny: clamp01(m.y) });
      break;
  }
}

// 떼는 줄이 끊겨 안 오면 발이 키를 누른 채 남는다. 오래 눌린 것은 놓는다.
function releasePeer(olderThan = 0) {
  for (const [i, p] of peerKeys) {
    if (now() - p.at < olderThan) continue;
    peerKeys.delete(i);
    handle({ t: 'k', vk: p.vk, d: false });
  }
  for (const b of [0, 1, 2]) if (buttons[b] && olderThan === 0) handle({ t: 'b', b, d: false });
}

const clamp01 = (v) => Math.min(1, Math.max(0, v));
const clamp1 = (v) => Math.min(1, Math.max(-1, v));

// 투명한 곳은 클릭이 뒤 창으로 빠져야 한다. 커서가 그림 위에 있을 때만 창이 마우스를 받는다.
function hitTest(lx, ly) {
  if (!win || locked) return;
  const inside = lx >= 0 && ly >= 0 && lx < innerWidth && ly < innerHeight;
  const el = inside ? document.elementFromPoint(lx, ly) : null;
  const over = !!el && el !== svg && svg.contains(el);
  setIgnore(!over);
}

function setIgnore(v) {
  if (v === ignoring) return;
  ignoring = v;
  win.setIgnoreCursorEvents(v).catch(() => {});
}

// ---------------------------------------------------------------- 표정

// 한참 이어 칠 때 얼굴. 캐릭터마다 사용자가 고른 원작 장면을 따라 그려 그림 위에 얹는다(그림은 그대로).
// 원작 장면은 이 그림보다 선이 가늘고 얼굴이 커서, 자리는 눈 사이 거리로 맞추고 선 굵기는 이 그림에 맞췄다.
const n2 = (v) => +v.toFixed(2);
const BURST = {
  // 미간의 효과선 넷과 그 아래 떨림 표시. 눈은 원작 장면처럼 흰 빛을 키워 다시 그렸더니 다른 아이처럼 보여서 그림 그대로 두고,
  // 입도 원작 장면과 같은 모양이라 둔다.
  hachiware(c) {
    const [[lx, ly], [rx, ry]] = c.eyes;
    const mx = (lx + rx) / 2, my = (ly + ry) / 2;
    // 왼쪽 둘을 그리고 미간을 축으로 뒤집는다. 선은 위쪽 가운데를 향해 살짝 휜다.
    const spark = (x0, y0, x1, y1, side) => {
      const X = (x) => n2(mx + side * x);
      const qx = (x0 + x1) / 2 + (y1 - y0) * 0.12, qy = (y0 + y1) / 2 - (x1 - x0) * 0.12;
      return `M ${X(x0)} ${n2(my + y0)} Q ${X(qx)} ${n2(my + qy)} ${X(x1)} ${n2(my + y1)}`;
    };
    const sparks = [1, -1].map((s) => spark(-14.2, -9, -10.6, -3.6, s) + ' ' + spark(-5.6, -9.4, -3.6, -3.4, s)).join(' ');
    const arch = (y) => `M ${n2(mx - 3.6)} ${n2(my + y + 0.6)} Q ${n2(mx)} ${n2(my + y - 1.4)} ${n2(mx + 3.6)} ${n2(my + y + 0.9)}`;
    return `<path class="line" style="stroke-width:1.9" d="${sparks}"/>` +
      `<path class="line" style="stroke-width:1.7" d="${arch(1.9)} ${arch(5.7)}"/>`;
  },
  // 공부하며 집중할 때 얼굴: 안쪽으로 내려간 눈썹과 다문 입 아래 작은 점. 눈과 입은 그림 그대로다.
  // 원래 둥근 눈썹은 눈에서 멀리 떠 있어서 덮고, 원작 장면처럼 눈 바로 위에 새로 긋는다.
  chiikawa(c) {
    const [[lx, ly], [rx, ry]] = c.eyes;
    const mx = (lx + rx) / 2, my = (ly + ry) / 2;
    const cover = (x, y) => `<ellipse class="skin" cx="${n2(x - 0.3)}" cy="${n2(y - 34)}" rx="8.6" ry="5.2"/>`;
    const brow = (s) => `M ${n2(mx + s * 35.1)} ${n2(my - 27.4)} L ${n2(mx + s * 23.9)} ${n2(my - 19)}`;
    return cover(lx, ly) + cover(rx, ry) +
      `<path class="line" style="stroke-width:3.6" d="${brow(-1)} ${brow(1)} ` +
      `M ${n2(mx - 0.4)} ${n2(my + 35.6)} L ${n2(mx - 0.4)} ${n2(my + 36.6)}"/>`;
  },
  // 신나서 꼼지락거릴 때의 입. 가운데가 솟고 오른쪽 아래로 꼬리가 내려온다. 벌린 입을 덮고 그 위에 그린다.
  // 벌린 입 아래 턱선은 입을 다물면 입이 둘인 것처럼 보여서 같이 덮는다(원작 장면에도 없다).
  usagi(c) {
    const [x, y, w, h] = c.mouth.rect;
    const k = 0.44;  // 원작 그림의 입 폭 60px 를 26 으로. 원작 장면처럼 눈에 비해 입이 조금 크다
    const sw = 3.8;
    return `<rect class="skin" x="${x}" y="${y}" width="${w}" height="${h}" rx="6"/>` +
      `<ellipse class="skin" cx="${n2(x + w / 2 - 0.3)}" cy="${n2(y + 35.2)}" rx="7.8" ry="4.6"/>` +
      `<path class="line" style="stroke-width:${n2(sw / k)}" ` +
      `transform="translate(${n2(x + w / 2)} ${n2(y + sw / 2)}) scale(${k}) translate(-394.5 -412.5)" ` +
      `d="M 364.5 422.5 Q 373.5 433.5 383.5 434.5 Q 392.5 434.5 396.5 426.5 L 396.5 412.5 ` +
      `M 396.5 426.5 Q 401.5 429.5 409.5 429.5 Q 418.5 428.5 424.5 418.5 ` +
      `M 408.5 429.5 Q 409.5 441.5 406.5 445.5 Q 404.5 448.5 399.5 448.5"/>`;
  },
};

let streakFrom = -1e9;
let burstTimer = 0;
function burst(on) {
  svg.classList.toggle('burst', on);
}

let sleepTimer = 0;
function active() {
  svg.classList.remove('sleep');
  clearTimeout(sleepTimer);
  sleepTimer = setTimeout(() => {
    svg.classList.add('sleep');
    kick();  // 꼬리가 처지도록
  }, SLEEP_MS);
}

// 가만히 있어도 고양이처럼 가끔 꼬리를 한 번 휘두른다. 늘 흔들면 창이 쉬지 않고 다시 그려진다.
function swishLoop() {
  setTimeout(() => {
    if (C.tail && !svg.classList.contains('sleep')) {
      flick(150);
      kick();
    }
    swishLoop();
  }, 5000 + Math.random() * 6000);
}

function blinkLoop() {
  setTimeout(() => {
    if (!svg.classList.contains('sleep')) {
      svg.classList.add('blink');
      setTimeout(() => svg.classList.remove('blink'), 110);
      if (Math.random() < 0.2) {
        setTimeout(() => svg.classList.add('blink'), 230);
        setTimeout(() => svg.classList.remove('blink'), 340);
      }
    }
    blinkLoop();
  }, 2200 + Math.random() * 4200);
}

// ---------------------------------------------------------------- 그리기

function setAttrs(el, attrs) {
  for (const k in attrs) el.setAttribute(k, typeof attrs[k] === 'number' ? attrs[k].toFixed(2) : attrs[k]);
}

function armRoot(side, px) {
  const s = SHOULDER[side];
  const sx = s.x + body.lean;
  const lo = ROOT_SLIDE[side][0] + body.lean;
  const hi = ROOT_SLIDE[side][1] + body.lean;
  const x = Math.min(hi, Math.max(lo, sx + (px - sx) * 0.55));
  return { x, y: s.y + 2 * Math.abs(x - sx) / (hi - lo) };
}

// 넓은 키(스페이스·시프트·엔터)는 가운데가 아니라 그 발에서 가까운 쪽을 친다
function hitX(side, key) {
  const r = armRoot(side, key.cx);
  return Math.min(key.x1 - 5, Math.max(key.x0 + 5, r.x + (side === 'L' ? 12 : -12)));
}

function drawArm(side, px, py, press) {
  const s = armRoot(side, px);
  const d = `M ${s.x.toFixed(2)} ${s.y.toFixed(2)} L ${px.toFixed(2)} ${py.toFixed(2)}`;
  $('arm' + side + 'o').setAttribute('d', d);
  $('arm' + side + 'w').setAttribute('d', d);
  // 발 크기는 하치와레 팔 굵기(17.4)에 맞춘 값을 캐릭터 팔 굵기만큼 늘이고 줄인다
  const k = C.arm.w / 17.4;
  const rx = (press ? 10.4 : 9.6) * k;
  const ry = (press ? 7.6 : 8.6) * k;
  setAttrs($('paw' + side + 'o'), { cx: px, cy: py, rx: rx + C.arm.line, ry: ry + C.arm.line });
  setAttrs($('paw' + side + 'w'), { cx: px, cy: py, rx, ry });
}

// 빛은 왼쪽 위에서 온다고 보고 그림자를 오른쪽 아래로 민다. 발이 뜰수록 그림자는 멀어지고 커지고 옅어진다.
function drawShadow(side, hidden) {
  const p = paw[side];
  const l = Math.max(0, p.lift);
  setAttrs($('shadow' + side), {
    cx: p.x + 1.5 + l * 0.3,
    cy: p.y + 12 + l * 0.9,
    rx: 10.5 + l * 0.5,
    ry: 3.6 + l * 0.18,
    opacity: hidden ? 0 : Math.max(0.14, 0.38 - l * 0.026),
  });
}

const fmt = (p) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`;

// 점들을 지나는 매끈한 곡선(Catmull-Rom 을 3차 베지어로)
function smoothPath(pts) {
  let d = `M ${fmt(pts[0])}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];
    const c1 = [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6];
    const c2 = [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6];
    d += ` C ${fmt(c1)} ${fmt(c2)} ${fmt(p2)}`;
  }
  return d;
}

function drawTail() {
  const pts = [[TAIL_BASE.x, TAIL_BASE.y]];
  TAIL_ANGLES.forEach((a, i) => {
    const r = (a + tail.w[i]) * Math.PI / 180;
    const [x, y] = pts[i];
    pts.push([x + TAIL_SEG * Math.cos(r), y + TAIL_SEG * Math.sin(r)]);
  });
  for (let i = 1; i < pts.length; i++) {
    const k = 1 + tail.env * (SWAY_OUT + SWAY * Math.sin(tail.phase - i * SWAY_WAVE));
    pts[i][0] = TAIL_BASE.x + (pts[i][0] - TAIL_BASE.x) * k;
  }
  const d = smoothPath(pts);
  $('tailO').setAttribute('d', d);
  $('tailB').setAttribute('d', d);
}

// 꼬리를 한 프레임 움직이고, 아직 흔들리는 중이면 true
function stepTail(dt, t) {
  // 치기 시작하면 흔들림이 서서히 커지고, 멈추면 서서히 잦아든다
  // 마우스로 넘어가면 바로 잦아든다. 몸이 마우스 쪽으로 끝까지 쏠린 채 바깥으로 흔들면 창 오른쪽 끝에 잘린다.
  const typing = t - lastKeyAt < SWAY_HOLD && lastKeyAt > lastMouseAct;
  tail.env += ((typing ? 1 : 0) - tail.env) * (1 - Math.exp(-dt / 350));
  tail.phase = (tail.phase + (dt / SWAY_MS) * 2 * Math.PI) % (2 * Math.PI);
  const target = svg.classList.contains('sleep') ? TAIL_SLEEP : 0;
  const s = dt / 1000;
  const a = -TAIL_W * TAIL_W * (tail.w[0] - target) - 2 * TAIL_DAMP * TAIL_W * tail.v;
  tail.v += a * s;
  tail.w[0] += tail.v * s;
  // 박자에 맞춰 연달아 튕기면 공진으로 점점 커지므로 끝에서 멈춘다
  const off = tail.w[0] - target;
  if (Math.abs(off) > TAIL_MAX) {
    tail.w[0] = target + Math.sign(off) * TAIL_MAX;
    if (tail.v * off > 0) tail.v = 0;
  }
  const k = 1 - Math.exp(-dt / TAIL_LAG);
  for (let i = 1; i < tail.w.length; i++) tail.w[i] += (tail.w[i - 1] - tail.w[i]) * k;
  drawTail();
  return tail.env > 0.01 || Math.abs(tail.v) > 0.3 || tail.w.some((w) => Math.abs(w - target) > 0.05);
}

const heldOn = (side) => {
  for (let i = held.length - 1; i >= 0; i--) if (held[i].key.side === side) return held[i].key;
  return null;
};

function pawTarget(side, t) {
  const k = heldOn(side);
  if (k) return { x: hitX(side, k), y: k.cy - PRESS_LIFT, press: true, tau: 16, kind: 'press' };
  const up = lastUp[side];
  // 오른발은 키를 치다가도 마우스를 만지면 바로 마우스로 돌아간다
  const typingAfterMouse = side === 'L' || up.at > lastMouseAct;
  if (up.key && t - up.at < HOVER_MS && typingAfterMouse) {
    return { x: hitX(side, up.key), y: up.key.cy - HOVER_LIFT, press: false, tau: 40, kind: 'hover' };
  }
  if (side === 'L') return { ...restL(), press: false, tau: 110, kind: 'rest' };
  const press = buttons[0] || buttons[1] || buttons[2];
  const onMouse = Math.hypot(paw.R.x - mouse.x, paw.R.y - gripY()) < 3;
  return { x: mouse.x, y: gripY() + (press ? 1.2 : 0), press, tau: onMouse ? 6 : 50, kind: 'mouse' };
}

function tiltTarget(t) {
  if (lastKey && t - lastKeyAt < 900) return { a: clamp1((lastKey.cx - KB_MID) / (KB.w / 2)) * TILT_MAX, tau: 120 };
  if (t - lastMoveAt < 1500) return { a: (clamp01(cursorNx) - 0.5) * 2 * TILT_MAX, tau: 220 };
  return { a: 0, tau: 600 };
}

function leanTarget(t) {
  if (lastKey && t - lastKeyAt < 900) return Math.min(8, Math.max(-8, (lastKey.cx - SPLIT_X) * 0.12));
  if (t - lastMouseAct < 1500) return Math.min(LEAN.max, Math.max(LEAN.min, (mouse.x - 300) * 0.6));
  return 0;
}

let raf = 0;
let lastT = 0;
function kick() {
  if (raf) return;
  lastT = now();
  raf = requestAnimationFrame(frame);
}

function frame(t) {
  const dt = Math.min(64, Math.max(1, t - lastT));
  lastT = t;

  const kM = 1 - Math.exp(-dt / 28);
  mouse.x += (mouseTarget.x - mouse.x) * kM;
  mouse.y += (mouseTarget.y - mouse.y) * kM;
  $('mouse').setAttribute('transform',
    `translate(${mouse.x.toFixed(2)} ${mouse.y.toFixed(2)}) scale(${floorScale(mouse.y).toFixed(3)})`);

  let moving = Math.abs(mouseTarget.x - mouse.x) + Math.abs(mouseTarget.y - mouse.y) > 0.05;
  for (const side of ['L', 'R']) {
    const P = pawTarget(side, t);
    const k = 1 - Math.exp(-dt / P.tau);
    paw[side].x += (P.x - paw[side].x) * k;
    paw[side].y += (P.y - paw[side].y) * k;
    drawArm(side, paw[side].x, paw[side].y, P.press);
    paw[side].lift += (LIFT[P.kind] - paw[side].lift) * (1 - Math.exp(-dt / 40));
    drawShadow(side, P.kind === 'mouse' && Math.abs(paw[side].lift - LIFT.mouse) < 0.5);
    if (Math.abs(P.x - paw[side].x) + Math.abs(P.y - paw[side].y) > 0.05) moving = true;
  }

  const B = tiltTarget(t);
  body.tilt += (B.a - body.tilt) * (1 - Math.exp(-dt / B.tau));
  body.nod *= Math.exp(-dt / 90);
  const lt = leanTarget(t);
  body.lean += (lt - body.lean) * (1 - Math.exp(-dt / 180));
  if (Math.abs(lt - body.lean) > 0.02) moving = true;
  const bodyT =
    `translate(${body.lean.toFixed(2)} ${body.nod.toFixed(2)}) rotate(${body.tilt.toFixed(3)} ${PIVOT.x} ${PIVOT.y})`;
  $('body').setAttribute('transform', bodyT);
  $('tail').setAttribute('transform', bodyT);
  if (C.tail && stepTail(dt, t)) moving = true;

  const settled = !moving &&
    t - lastUp.L.at > HOVER_MS + 50 && t - lastUp.R.at > HOVER_MS + 50 &&
    Math.abs(B.a - body.tilt) < 0.01 && body.nod < 0.01 &&
    t - lastKeyAt > 950 && t - lastMoveAt > 1550;
  raf = settled ? 0 : requestAnimationFrame(frame);
}

// ---------------------------------------------------------------- 이름표

// 이름표 자리. 그림이 창 위끝까지 차 있어서 창을 이만큼 위로 늘렸다(main.rs 의 TAG_ROOM 과 같아야 한다).
// 몸이 4도까지 기울면 긴 이름의 끝이 그만큼 더 올라가서, 그때도 안 잘릴 만큼 둔다.
const TAG_ROOM = 26;
// 글자 가운데가 머리 꼭대기에서 이만큼 위. 글자는 머리보다 작게.
const TAG_LIFT = 12;
let tagName = '';
let tagOn = true;

/** 줄바꿈·제어 문자를 빼고 열 글자까지(nametag.rs 의 clean 과 같다). 상대가 보낸 이름도 여기를 거친다. */
const cleanName = (s) =>
  Array.from(String(s ?? '').replace(/[\u0000-\u001f\u007f-\u009f]/g, ' ').replace(/\s+/g, ' ').trim()).slice(0, 10).join('');

/** 실루엣(hit) 맨 위 언저리의 가운데와 꼭대기 높이. 하치와레는 머리가 그림 가운데보다 오른쪽에 있다. */
function headOf(c) {
  const n = c.hit.match(/-?\d+(?:\.\d+)?/g).map(Number);
  let top = Infinity;
  for (let i = 1; i < n.length; i += 2) top = Math.min(top, n[i]);
  const xs = [];
  for (let i = 0; i < n.length; i += 2) if (n[i + 1] < top + 6) xs.push(n[i]);
  return [(Math.min(...xs) + Math.max(...xs)) / 2, top];
}

// 머리 위 가운데에 글자만 띄운다 — 졸 때 뜨는 Z 와 같은 결(캐릭터 색 글자에 검은 테, style.css).
// 몸(#body) 안에 있어서 몸이 기울면 같이 기운다. 클릭은 받지 않는다 — 이름 위는 뒤 창을 눌러야 한다.
function drawTag() {
  const show = tagOn && tagName !== '';
  $('tag').style.display = show ? '' : 'none';
  if (!show) return;
  C.head ??= headOf(C);
  const [cx, top] = C.head;
  const text = $('tagText');
  text.textContent = tagName;
  setAttrs(text, { x: n2(cx), y: n2(top - TAG_LIFT) });
}

function setTag(name, on = tagOn) {
  tagName = cleanName(name);
  tagOn = !!on;
  drawTag();
}

// ---------------------------------------------------------------- 캐릭터

// 캐릭터마다 다른 것은 그림과 그 그림에서 잰 자리(chars.js)뿐이다. 그림을 팔이 몸에서 나오는 자리가 하치와레와
// 같은 곳에 오게 앉혀 두어서 키보드·발·마우스·몸 기울기 계산은 셋이 같이 쓴다.
// 창 아래 끝(책상 앞)은 274 로 고정이고, 귀가 긴 캐릭터는 창이 위로 늘어난다(main.rs 의 CHARS 높이와 같다).
// 그 위로 이름표 자리(TAG_ROOM)가 더 붙는다.
function setChar(key) {
  if (!Object.hasOwn(CHARS, key)) key = 'hachiware';
  C = CHARS[key];
  document.documentElement.dataset.char = key;
  svg.setAttribute('viewBox', `72 ${C.top - TAG_ROOM} 320 ${274 - C.top + TAG_ROOM}`);
  const st = document.documentElement.style;
  st.setProperty('--skin', C.skin);
  st.setProperty('--line', C.line);
  st.setProperty('--eye-ink', C.eyeInk);
  st.setProperty('--arm-w', C.arm.w);
  st.setProperty('--arm-o', C.arm.w + 2 * C.arm.line);
  const [x, y, w] = C.box;
  setAttrs($('cat'), { href: C.img, x, y, width: w, height: w });
  $('hit').setAttribute('d', C.hit);
  C.eyes.forEach(([cx, cy], i) => setAttrs($(i ? 'eyeCoverR' : 'eyeCoverL'), { cx, cy, rx: C.eyeR, ry: C.eyeR }));
  $('eyeLines').setAttribute('d', C.closedEye);
  const m = C.mouth;
  setAttrs($('mouthCover'), m ? { x: m.rect[0], y: m.rect[1], width: m.rect[2], height: m.rect[3] } : { width: 0, height: 0 });
  $('mouthLine').setAttribute('d', m ? m.path : '');
  $('burst').innerHTML = BURST[key] ? BURST[key](C) : '';
  $('tail').style.display = C.tail ? '' : 'none';
  $('zzz').setAttribute('transform', `translate(${C.z} 0)`);
  drawTag();
  kick();
}

// ---------------------------------------------------------------- 시작

window.hachi = { handle, setChar, setTag, fromPeer, releasePeer };

// 캐릭터 위에서 휠을 굴리면 크기를 바꾼다(main.rs 의 zoom). 휠 한 칸(100)에 약 15%.
// 터치패드는 잘게 여러 번 오므로 한 프레임치를 모아 한 번에 보낸다. 클릭 통과(고정)일 때는 창이 휠을 받지 않는다.
let wheelSum = 0;
let wheelRaf = 0;
function onWheel(e) {
  e.preventDefault();
  // 끌기처럼 그림 위에서만. 투명한 곳은 원래 뒤 창 몫이다.
  if (e.target === svg) return;
  wheelSum += e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
  if (wheelRaf) return;
  wheelRaf = requestAnimationFrame(() => {
    wheelRaf = 0;
    const factor = Math.exp(-wheelSum * 0.0015);
    wheelSum = 0;
    T.core.invoke('zoom', { factor }).catch(() => {});
  });
}
if (T) svg.addEventListener('wheel', onWheel, { passive: false });
document.addEventListener('contextmenu', (e) => e.preventDefault());
const params = new URLSearchParams(location.search);
setChar(PEER ? HACHI.char : params.get('char'));
// 브라우저로 열어 모양을 볼 때는 ?name= 으로 이름표를 띄운다
if (!T) setTag(params.get('name'));
blinkLoop();
swishLoop();
active();
kick();

if (T && PEER) {
  win = T.window.getCurrentWindow();
  svg.addEventListener('mousedown', (e) => {
    if (e.button === 0 && e.target !== svg) win.startDragging().catch(() => {});
  });
  on('hover', ({ payload }) => handle(payload));
  on('peer', ({ payload }) => fromPeer(payload));
  on('char', ({ payload }) => setChar(payload));
  on('lock', ({ payload }) => {
    locked = payload;
    ignoring = true;
  });
  // 상대 이름은 창이 뜨기 전에 와 있을 수 있어서 Rust 가 들고 있다가 ready 로 준다. 듣기를 먼저 걸어야
  // ready 와 그 뒤 바뀐 이름 사이에 빈틈이 없다.
  Promise.all([
    on('name', ({ payload }) => setTag(payload)),
    on('tag', ({ payload }) => setTag(tagName, payload)),
  ])
    .then(() => T.core.invoke('ready'))
    .then((s) => {
      locked = s.passthrough;
      setTag(s.peer_name, s.nametag);
    })
    .catch(() => {});
  setInterval(() => releasePeer(4000), 1000);
} else if (T) {
  // 저장된 캐릭터를 받기 전에 하치와레가 한 번 비쳤다가 바뀌지 않게 가려 둔다.
  // 답이 끝내 안 와도 빈 창으로 남지 않게 잠시 뒤엔 무조건 보인다(한 번 원인 모르게 빈 창으로 떴다).
  svg.style.visibility = 'hidden';
  setTimeout(() => { svg.style.visibility = ''; }, 1500);
  win = T.window.getCurrentWindow();
  svg.addEventListener('mousedown', (e) => {
    if (e.button === 0 && e.target !== svg) win.startDragging().catch(() => {});
  });
  on('input', ({ payload }) => {
    handle(payload);
    together.share(payload);
  });
  on('lock', ({ payload }) => {
    locked = payload;
    ignoring = true;
  });
  on('char', ({ payload }) => {
    setChar(payload);
    together.hello();
  });
  on('room', ({ payload }) => together.join(payload));
  on('name', ({ payload }) => {
    setTag(payload);
    together.hello();
  });
  on('tag', ({ payload }) => setTag(tagName, payload));
  T.core.invoke('ready')
    .then((s) => {
      locked = s.passthrough;
      setChar(s.character);
      setTag(s.name, s.nametag);
      together.start(s);
    })
    .catch(() => {})
    .finally(() => { svg.style.visibility = ''; });
} else if (!params.has('still')) {
  demo();
}

// 브라우저에서 열면 혼자 움직인다. 앱 밖에서 모양을 확인할 때 쓴다.
// 치기와 마우스를 번갈아 한다. 마우스를 만지면 꼬리 살랑임이 잦아들어서, 둘을 같이 돌리면 살랑임이 안 보인다.
// 12초 한 바퀴: 0~6 치기(3초부터 이어 칠 때 얼굴), 6~10 마우스, 나머지는 쉬기.
function demo() {
  const t0 = now();
  const at = () => ((now() - t0) / 1000) % 12;
  setInterval(() => {
    const p = at();
    if (p < 6 || p > 10) return;
    const s = (now() - t0) / 1000;
    handle({ t: 'm', nx: 0.5 + 0.45 * Math.sin(s * 1.3), ny: 0.5 + 0.45 * Math.sin(s * 2.1 + 1), lx: -1, ly: -1 });
  }, 16);
  const vks = [...'HACHIWARE'].map((c) => c.charCodeAt(0)).concat([0x20, 0x0d, 0x08, 0x26]);
  let i = 0;
  setInterval(() => {
    if (at() >= 6) return;
    const vk = vks[i++ % vks.length];
    handle({ t: 'k', vk, d: true });
    setTimeout(() => handle({ t: 'k', vk, d: false }), 90);
  }, 200);
  setInterval(() => {
    const p = at();
    if (p < 6.5 || p > 9.8) return;
    handle({ t: 'b', b: 0, d: true });
    setTimeout(() => handle({ t: 'b', b: 0, d: false }), 120);
  }, 900);
}
