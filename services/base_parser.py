from zstandard import ZstdDecompressor
from pathlib import Path
import json
import os
import string
import random
import re

import codecs


# Открываю файл 1 раз и начинаю писать порциями

def generate_random_string(length=8):
    characters = string.ascii_letters + string.digits
    random_string = ''.join(random.choices(characters, k=length))
    
    return random_string

def write_single_pgn(pgn_str: str) -> None:
    pgn_folder = Path("output").resolve()
    pgn_folder.mkdir(exist_ok=True)
    
    file_path = Path(pgn_folder / (generate_random_string(8) + ".pgn"))
    
    with open(file_path, "w", encoding="utf-8") as f:
        f.write(pgn_str)


def parse_chunk(data, tail, decoder) -> list[str] | str:
    """Парсит чанк."""
    result = decoder.decode(data)
    
    parts: list[str] = result.split('\n\n[')
    last_part: str = ""
    pgns: list[str] = []

    total_parts = len(parts) # Вынес за тело цикла

    for index, game in enumerate(parts):
        if (index == 0):
            game = tail + game
            # print("Glued game: ", game)
            
        if not game.startswith('['):
            game = '[' + game 
        
        if (index == total_parts - 1):
            last_game = game
            break

        pgns.append(game)
    
    return pgns, last_game 

def check_game_matches(pgn: str, flags: tuple[str, int, int, str]) -> bool:
    """Проверяет игру по фильтрам."""

    time_control_pattern = r'\[Event \"Rated ([A-Z][a-z]*) [Gg]ame\"\]'
    black_elo_pattern = r'\[WhiteElo \"(\d*)\"\]'
    white_elo_pattern = r'\[BlackElo \"(\d*)\"\]'
    eco_pattern = r'\[ECO \"(\w*)\"\]'

    time_control_req, black_elo_req, white_elo_req, eco_req = flags

    time_control = re.search(time_control_pattern, pgn)
    black_elo = re.search(black_elo_pattern, pgn)
    white_elo = re.search(white_elo_pattern, pgn)
    eco = re.search(eco_pattern, pgn) 

    # Если игра неполная - пропускаем
    if not all((time_control, black_elo, white_elo, eco)):
        return False

    return (
            (time_control.group(1) == time_control_req) and
            (int(black_elo.group(1)) >= black_elo_req) and
            (int(white_elo.group(1)) >= white_elo_req) and
            (eco.group(1) in eco_req)
    )


def decompress_database(path: str) -> None:    
    """Осуществляет потоковую распаковку базы данных."""
    chunks_total = 0
    time_control_req = "Rapid" 
    black_elo_min = 2200
    white_elo_min = 2200
    english_eco = [f"A{code}" for code in range(10, 40)]

    flags = (time_control_req, black_elo_min, white_elo_min, english_eco)
    
    
    with open(path, "rb") as fh:
        cctx = ZstdDecompressor(max_window_size=2**31)
        reader = cctx.stream_reader(fh, os.stat(path).st_size) # ZstdCompressionReader
        
        decoder = codecs.getincrementaldecoder('utf-8')()
        
        with open("output_4.pgn", 'w', encoding='utf-8') as f:
            tail: str = ""

            games_collected: int = 0
            
            while True:
                chunk = reader.read(1638400) 
               
                if not chunk:  
                    break

                chunks_total += 1
                print(f"============   Chunks readed: {chunks_total} ============")

                games, tail = parse_chunk(chunk, tail, decoder)

                for game in games:
                    if check_game_matches(game, flags):
                        games_collected += 1
                        f.write(game)
                        f.write('\n\n')
                
                if (chunks_total == 10000):
                    break

        print(f"Decompression finished! Total games collected: {games_collected}")

if __name__ == "__main__":

    config_path = str(Path(__file__).resolve().parent.parent/"config.json")
    
    with open(config_path, "r", encoding="utf-8") as f:
        data: dict = json.load(f)
        lichess_db_path = data.get('GAME_DATABASE_PATH')

        if not lichess_db_path:
            print("Base not found")
        else:
            decompress_database(lichess_db_path)

            

