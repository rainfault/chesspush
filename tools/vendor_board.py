"""Fetch pinned upstream assets; never required at application startup."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import hashlib
import json
import urllib.request

ROOT = Path(__file__).resolve().parents[1] / 'ui' / 'web' / 'vendor'
VERSION = '10.4.0'
LILA = 'fa6614fcd5bc8afdad24bf341dbc6686afa4208d'
CG = f'https://cdn.jsdelivr.net/npm/@lichess-org/chessground@{VERSION}'
LI = f'https://raw.githubusercontent.com/lichess-org/lila/{LILA}'
FILES = {
    'chessground.js': f'{CG}/dist/chessground.min.js',
    'chessground.base.css': f'{CG}/assets/chessground.base.css',
    'chessground.brown.css': f'{CG}/assets/chessground.brown.css',
    'CHESSGROUND-LICENSE': f'{CG}/LICENSE',
    'LILA-COPYING.md': f'{LI}/COPYING.md',
    'brown.png': f'{LI}/public/images/board/brown.png',
    'lichess.ttf': f'{LI}/public/font/lichess.ttf',
    'lichess.sfd': f'{LI}/public/font/lichess.sfd',
    'licon.ts': f'{LI}/ui/lib/src/licon.ts',
    'GPL-2.0.txt': 'https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt',
    'AGPL-3.0.txt': 'https://www.gnu.org/licenses/agpl-3.0.txt',
}
for color in 'wb':
    for piece in 'KQRBNP':
        FILES[f'pieces/{color}{piece}.svg'] = f'{LI}/public/piece/cburnett/{color}{piece}.svg'
# Qt reads TrueType directly; the same Noto Sans family used by Lichess.
FILES['NotoSans.ttf'] = 'https://raw.githubusercontent.com/notofonts/noto-fonts/v20201206-phase3/hinted/ttf/NotoSans/NotoSans-Regular.ttf'
FILES['NOTO-LICENSE'] = 'https://raw.githubusercontent.com/notofonts/noto-fonts/v20201206-phase3/LICENSE'

def fetch(item):
    name, url = item
    data = urllib.request.urlopen(url, timeout=45).read()
    path = ROOT / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {'file': name, 'url': url, 'sha256': hashlib.sha256(data).hexdigest()}

if __name__ == '__main__':
    with ThreadPoolExecutor(max_workers=6) as pool:
        records = list(pool.map(fetch, FILES.items()))
    (ROOT / 'manifest.json').write_text(json.dumps(records, indent=2), encoding='utf-8')
    print(f'Vendored {len(records)} files, Chessground {VERSION}')
