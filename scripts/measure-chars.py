"""치이스페 얼굴 그림에서 캐릭터마다 다른 자리를 재서 src/chars.js 를 만든다.

    python -I scripts/measure-chars.py

손으로 잰 숫자는 그림이 바뀌면 조용히 어긋난다. 그래서 그림에서 다시 잴 수 있게 남긴다.
그림은 자르거나 덧칠하지 않는다. 760px 로 줄여 src/ 에 둘 뿐이다(이미 있으면 두고 간다).

장면에 앉히는 기준은 팔이 몸에서 나오는 오목한 두 점이다. 하치와레 그림의 그 두 점이 장면에서 놓인 자리에
다른 캐릭터의 두 점을 맞춘다. 그래야 발·키보드·마우스 계산을 캐릭터마다 다시 하지 않아도 팔이 그림 속 팔 자리에서 나온다.
"""

import json
import math
import os
from collections import deque

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, '..', 'src')
FACES = os.path.join(HERE, '..', '..', 'kasaspace', 'ui', 'assets', 'faces')

# 하치와레 그림을 장면에 앉힌 자리(index.html 첫 판). 다른 캐릭터는 여기 맞춰 앉힌다.
HACHI_BOX = (70.86, -8.23, 265.47)
DESK_Y = 185.5  # 책상 윗면 뒤 끝. 이 아래는 책상이 가린다.

# 재서 알 수 없는 것만 손으로 정한다.
#  arm: 그려 붙이는 팔의 흰 줄기 굵기와 외곽선 굵기(한쪽). 그림 속 팔·선 굵기를 장면 배율로 옮긴 값에 맞췄다.
#  z: 졸 때 뜨는 z 의 가로 자리 보정. 머리 오른쪽 윤곽에 겹치지 않게.
#  mouth: 졸 때 입을 다문 모양으로 덮을지. 치이카와는 원래 다문 입이라 덮지 않는다.
#  tail: 화면엔 머리만 보이니 책상 위로 올라올 만큼 긴 꼬리만 그린다. 치이카와·우사기 꼬리는 짧아 책상에 가린다.
CHARS = [
    {'slug': 'hachiware', 'name': '하치와레', 'arm': (17.4, 4.8), 'z': 0, 'mouth': True, 'tail': True},
    {'slug': 'chiikawa', 'name': '치이카와', 'arm': (19.2, 4.8), 'z': 8, 'mouth': False, 'tail': False},
    {'slug': 'usagi', 'name': '우사기', 'arm': (17.4, 5.4), 'z': 5, 'mouth': True, 'tail': False},
]


def lum(p):
    return 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]


def hexc(c):
    return '#%02x%02x%02x' % tuple(c[:3])


def median(cs):
    cs = sorted(cs, key=lum)
    return cs[len(cs) // 2]


def edges(px, W, H):
    L, R = {}, {}
    for y in range(H):
        xs = [x for x in range(W) if px[x, y][3] > 128]
        if xs:
            L[y], R[y] = xs[0], xs[-1]
    return L, R


def corners(L, R, H):
    """머리 가장 넓은 줄에서 아래로 내려가며, 몸이 좁아지다가 팔 때문에 다시 넓어지기 직전 줄"""
    widest = max((y for y in L if y < H * 0.75), key=lambda y: R[y] - L[y])

    def find(edge, sign):
        best, by = None, None
        for y in range(widest, H):
            v = sign * edge[y]
            if best is None or v > best:
                best, by = v, y
            elif v < best - 15:
                return edge[by], by
        raise ValueError('팔 자리를 못 찾음')

    return find(L, 1), find(R, -1)


def blobs(im):
    """어두운 덩어리(눈·입·눈썹). 반으로 줄여 센다."""
    s = 2
    W, H = im.size
    sw, sh = W // s, H // s
    px = im.resize((sw, sh), Image.BILINEAR).load()
    dark = [[px[x, y][3] > 200 and lum(px[x, y]) < 90 for x in range(sw)] for y in range(sh)]
    seen = [[False] * sw for _ in range(sh)]
    out = []
    for y in range(sh):
        for x in range(sw):
            if not dark[y][x] or seen[y][x]:
                continue
            q = deque([(x, y)])
            seen[y][x] = True
            n, x0, y0, x1, y1 = 0, x, y, x, y
            while q:
                cx, cy = q.popleft()
                n += 1
                x0, y0, x1, y1 = min(x0, cx), min(y0, cy), max(x1, cx), max(y1, cy)
                for nx, ny in ((cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)):
                    if 0 <= nx < sw and 0 <= ny < sh and dark[ny][nx] and not seen[ny][nx]:
                        seen[ny][nx] = True
                        q.append((nx, ny))
            w, h = (x1 - x0 + 1) * s, (y1 - y0 + 1) * s
            if n > 40 and w < W * 0.5:
                out.append({'x0': x0 * s, 'y0': y0 * s, 'x1': (x1 + 1) * s, 'y1': (y1 + 1) * s,
                            'w': w, 'h': h, 'fill': n * s * s / (w * h),
                            'cx': (x0 + x1 + 1) * s / 2, 'cy': (y0 + y1 + 1) * s / 2})
    return out


def eyes_and_mouth(bs):
    # 눈은 꽉 찬 동그라미 둘이 같은 높이에 있다. 눈썹 호는 상자가 네모여도 속이 비어 걸러진다.
    round_ = [b for b in bs if b['fill'] > 0.55 and 0.8 < b['w'] / b['h'] < 1.25 and b['w'] > 70]
    pairs = [(a, b) for a in round_ for b in round_ if a['cx'] < b['cx'] and abs(a['cy'] - b['cy']) < 15]
    l, r = max(pairs, key=lambda p: p[0]['w'] * p[0]['h'] + p[1]['w'] * p[1]['h'])
    mid = (l['cx'] + r['cx']) / 2
    below = [b for b in bs if abs(b['cx'] - mid) < 40 and b['cy'] > l['cy'] and b['w'] > 60]
    mouth = min(below, key=lambda b: b['cy'])
    return l, r, mouth


def ring_color(px, cx, cy, r):
    # 채널마다 따로 가운데 값을 쓴다. 밝기로 고른 한 픽셀은 눈 가장자리 번진 색이 섞여 덮개가 옅은 원으로 비쳤다.
    cs = []
    for k in range(72):
        a = 2 * math.pi * k / 72
        for d in (r + 10, r + 14, r + 18):
            p = px[int(cx + d * math.cos(a)), int(cy + d * math.sin(a))]
            if p[3] > 250 and lum(p) > 150 and max(p[:3]) - min(p[:3]) < 60:
                cs.append(p)
    return tuple(sorted(c[i] for c in cs)[len(cs) // 2] for i in range(3))


def r1(v):
    return round(v, 2)


def measure(c, ref):
    im = Image.open(os.path.join(FACES, c['slug'] + '.png')).convert('RGBA')
    W, H = im.size
    px = im.load()
    L, R = edges(px, W, H)
    (lx, ly), (rx, ry) = corners(L, R, H)
    if ref is None:
        x0, y0, size = HACHI_BOX
        s = size / W
        ref = ((x0 + lx * s, y0 + ly * s), (x0 + rx * s, y0 + ry * s))
    else:
        (tlx, tly), (trx, try_) = ref
        s = (trx - tlx) / (rx - lx)
        x0 = tlx - lx * s
        y0 = (tly + try_) / 2 - (ly + ry) / 2 * s
    to = lambda x, y: (x0 + x * s, y0 + y * s)

    l, r, mouth = eyes_and_mouth(blobs(im))
    eye_r = (l['w'] + l['h'] + r['w'] + r['h']) / 8
    # 감은 눈은 눈 폭의 9 할 길이로, 아래로 처진 호. 하치와레 첫 판에서 눈으로 맞춘 비율이다.
    half = eye_r * s * 0.9
    eyes = [to(l['cx'], l['cy']), to(r['cx'], r['cy'])]
    skin = ring_color(px, l['cx'], l['cy'], eye_r)
    eye_ink = median([px[x, y] for x in range(int(l['x0']), int(l['x1'])) for y in range(int(l['y0']), int(l['y1']))
                      if px[x, y][3] > 250 and lum(px[x, y]) < 60])
    line = median([px[x, y] for y in range(H // 2, H // 2 + 120) for x in range(L[y], L[y] + 18)
                   if px[x, y][3] > 250 and lum(px[x, y]) < 70])

    out = {
        'name': c['name'],
        'img': c['slug'] + '.png',
        'box': [r1(x0), r1(y0), r1(W * s)],
        'skin': hexc(skin),
        'line': hexc(line),
        'eyeInk': hexc(eye_ink),
        'eyes': [[r1(x), r1(y)] for x, y in eyes],
        'eyeR': r1(eye_r * s + 2),
        'closedEye': ' '.join(
            f'M {r1(x - half)} {r1(y)} Q {r1(x)} {r1(y + half * 0.714)} {r1(x + half)} {r1(y)}' for x, y in eyes),
        'arm': {'w': c['arm'][0], 'line': c['arm'][1]},
        'z': c['z'],
        'tail': c['tail'],
    }

    if c['mouth']:
        mx0, my0 = to(mouth['x0'], mouth['y0'])
        mx1, my1 = to(mouth['x1'], mouth['y1'])
        # 다문 입은 하치와레 첫 판의 ω 를 입 상자 위쪽에 맞춰 옮긴다(입 상자 왼쪽 위 기준).
        w = mx1 - mx0
        k = w / 25.8
        pts = [(1.1, 3.5), (2.7, 9.6), (7.5, 9.4), (11.7, 9.0), (13.1, 3.2), (14.5, 9.0), (18.7, 9.4), (23.5, 9.6), (25.1, 3.5)]
        P = [(r1(mx0 + dx * k), r1(my0 + dy * k)) for dx, dy in pts]
        out['mouth'] = {
            'rect': [r1(mx0 - 1.4), r1(my0 - 1.5), r1(w + 2.8), r1(my1 - my0 + 3)],
            'path': (f'M {P[0][0]} {P[0][1]} Q {P[1][0]} {P[1][1]} {P[2][0]} {P[2][1]} '
                     f'Q {P[3][0]} {P[3][1]} {P[4][0]} {P[4][1]} Q {P[5][0]} {P[5][1]} {P[6][0]} {P[6][1]} '
                     f'Q {P[7][0]} {P[7][1]} {P[8][0]} {P[8][1]}'),
        }
    else:
        out['mouth'] = None

    # 클릭을 받는 실루엣. 책상 위로 보이는 부분만, 줄마다 왼쪽·오른쪽 끝을 이은 것.
    top = min(L)
    rows = []
    sy = y0 + top * s
    while sy < DESK_Y:
        rows.append(sy)
        sy += 5.1
    rows.append(DESK_Y)
    left, right = [], []
    for sy in rows:
        y = min(H - 1, max(top, round((sy - y0) / s)))
        left.append((r1(x0 + L[y] * s), r1(sy)))
        right.append((r1(x0 + R[y] * s), r1(sy)))
    poly = left + right[::-1]
    out['hit'] = 'M ' + ' L '.join(f'{x} {y}' for x, y in poly) + ' Z'

    # 귀 끝 위로 여백을 두고 창 위쪽을 잡는다. 하치와레·치이카와는 원래 창(0)에 들어간다.
    ear = y0 + top * s
    out['top'] = min(0, math.floor(ear - 11))

    dst = os.path.join(SRC, c['slug'] + '.png')
    if not os.path.exists(dst):
        im.resize((760, 760), Image.LANCZOS).save(dst, optimize=True)
    print(f"{c['slug']:10} scale {s:.4f}  corners L({lx},{ly}) R({rx},{ry})  ear {ear:.1f}  top {out['top']}  "
          f"skin {out['skin']}  line {out['line']}  eye {out['eyeInk']}")
    return out, ref


def main():
    ref = None
    data = {}
    for c in CHARS:
        data[c['slug']], ref = measure(c, ref)
    lines = []
    for slug, d in data.items():
        lines.append(f'  {slug}: {{')
        lines += [f'    {k}: {json.dumps(v, ensure_ascii=False)},' for k, v in d.items()]
        lines.append('  },')
    with open(os.path.join(SRC, 'chars.js'), 'w', encoding='utf-8', newline='\n') as f:
        f.write('// scripts/measure-chars.py 가 치이스페 그림에서 재서 만든 파일. 고칠 때는 스크립트를 고치고 다시 돌린다.\n')
        f.write("'use strict';\nconst CHARS = {\n" + '\n'.join(lines) + '\n};\n')
    # main.rs 의 CHARS 창 높이와 같아야 한다
    print('BASE_H', {k: 274 - v['top'] for k, v in data.items()})


main()
