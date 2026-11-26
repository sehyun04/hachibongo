// 판 번호를 한곳에서 바꾼다. `node scripts/version.mjs 0.2.1`
// 앱이 업데이트 때 견주는 번호는 tauri.conf.json 것인데, 나머지 셋이 어긋나 있으면 어느 게 진짜인지 헷갈려서 같이 맞춘다.
// CI 는 `--check <ref_type> <ref_name>` 로 넷이 같은지, 태그가 그 번호인지 본다 — 번호를 안 올리고 태그만 달면 앱들이 새 판을 못 알아본다.
import { readFileSync, writeFileSync } from 'node:fs';

const root = new URL('..', import.meta.url);
const read = (p) => readFileSync(new URL(p, root), 'utf8');
const write = (p, s) => writeFileSync(new URL(p, root), s);

// 파일마다 [읽기, 쓰기]. 쓰기는 글 모양을 그대로 두려고 번호 자리만 바꾼다.
const files = {
  'src-tauri/tauri.conf.json': [(s) => JSON.parse(s).version, (s, v) => s.replace(/("version":\s*)"[^"]+"/, `$1"${v}"`)],
  'package.json': [(s) => JSON.parse(s).version, (s, v) => s.replace(/("version":\s*)"[^"]+"/, `$1"${v}"`)],
  'package-lock.json': [
    (s) => {
      const j = JSON.parse(s);
      return j.version === j.packages[''].version ? j.version : `${j.version}/${j.packages[''].version}`;
    },
    (s, v) => s.replace(/("version":\s*)"[^"]+"/, `$1"${v}"`).replace(/("packages":\s*\{\s*"":\s*\{[^}]*?"version":\s*)"[^"]+"/, `$1"${v}"`),
  ],
  'src-tauri/Cargo.toml': [(s) => /^\[package\][^[]*?^version = "([^"]+)"/m.exec(s)?.[1], (s, v) => s.replace(/(^\[package\][^[]*?^version = )"[^"]+"/m, `$1"${v}"`)],
  // 윈도우 CI 는 줄 끝을 CRLF 로 받아 온다
  'src-tauri/Cargo.lock': [(s) => /name = "hachibongo"\r?\nversion = "([^"]+)"/.exec(s)?.[1], (s, v) => s.replace(/(name = "hachibongo"\r?\nversion = )"[^"]+"/, `$1"${v}"`)],
};
const now = () => Object.entries(files).map(([p, [get]]) => [p, get(read(p))]);

const [arg, refType, refName] = process.argv.slice(2);
if (arg === '--check') {
  const seen = now();
  const v = seen[0][1];
  const off = seen.filter(([, x]) => x !== v);
  for (const [p, x] of off) console.error(`${p} 는 ${x}, tauri.conf.json 은 ${v}`);
  if (refType === 'tag' && refName !== `v${v}`) {
    console.error(`태그 ${refName} 와 판 번호 ${v} 가 다르다`);
    process.exit(1);
  }
  if (off.length) process.exit(1);
  console.log(`판 ${v}`);
} else if (/^\d+\.\d+\.\d+$/.test(arg ?? '')) {
  for (const [p, [, set]] of Object.entries(files)) write(p, set(read(p), arg));
  for (const [p, x] of now()) console.log(`${x}  ${p}`);
} else {
  console.error('쓰는 법: node scripts/version.mjs 0.2.1   |   node scripts/version.mjs --check tag v0.2.1');
  process.exit(1);
}
