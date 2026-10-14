"""Reproducible synthetic performance input and Python reference timing."""
from pathlib import Path
import sys,time,json,zstandard
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from backend.search.model_game_search import iter_matching_games_from_path
from backend.filters.game_filters import GameFilter
target=ROOT/'v2' / '.qa' / 'benchmark.pgn.zst'
target.parent.mkdir(exist_ok=True)
block=(Path(__file__).parent/'fixtures'/'corpus.pgn').read_bytes()
with target.open('wb') as f,zstandard.ZstdCompressor().stream_writer(f) as stream:
    for _ in range(10000):stream.write(block)
progress=[0];found=0
started=time.perf_counter()
for i,game in iter_matching_games_from_path(target,GameFilter(),progress_callback=lambda n:progress.__setitem__(0,n)):
    found+=1
elapsed=time.perf_counter()-started
print(json.dumps(dict(implementation='python-reference',scanned=progress[0],matched=found,seconds=elapsed,games_per_second=progress[0]/elapsed)))
