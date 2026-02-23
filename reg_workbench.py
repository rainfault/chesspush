import re

data = "12345 [ECO 'A40'] DDD some [ECO 'A40'] data"
data_second = "[ECO 'A40'] 12345 "
pattern = r"\[ECO \'A40\'\]"

pgn_pattern=r'\[Event \"Rated [Blitz|Rapid] game\"\]*\[ECO \"A40"\]*'

time_control_pattern = r'\[Event \"Rated ([A-Z][a-z]*) Game\"\]'
black_elo_pattern = r'\[WhiteElo \"(\d*)\"\]'
white_elo_pattern = r'\[BlackElo \"(\d*)\"\]'
eco_pattern = r'\[ECO \"(\w*)\"\]'

pgn_sample = """
    [Event "Rated Blitz Game"]
    [Site "https://lichess.org/8LwiNwZm"]
    [Date "2025.12.01"]
    [Round "-"]
    [White "Izamutdin"]
    [Black "Raximov_Toshpolat"]
    [Result "0-1"]
    [UTCDate "2025.12.01"]
    [UTCTime "00:00:13"]
    [WhiteElo "1811"]
    [BlackElo "1831"]
    [WhiteRatingDiff "-6"]
    [BlackRatingDiff "+5"]
    [ECO "A40"]
    [Opening "Queen's Pawn Game"]
    [TimeControl "300+0"]
    [Termination "Normal"]

    1. d4 { [%clk 0:05:00] } 1... c6 { [%clk 0:05:00] } 2. e3 { [%clk 0:05:00] } 2... e6
"""

time_control = re.search(time_control_pattern, pgn_sample)
black_elo = re.search(black_elo_pattern, pgn_sample)
white_elo = re.search(white_elo_pattern, pgn_sample)
eco = re.search(eco_pattern, pgn_sample)

print(time_control.group(1), black_elo.group(1), white_elo.group(1), eco.group(1))

english_eco = [f"A{code}" for code in range(10, 40)]
print(english_eco)
#  Проверка на {
#                   ECO
#                   WhiteElo
#                   BlackElo
#                   Event (Blitz / Rapid)
#              }

# found = re.findall(pattern, data)
# print(found)

# search = re.search(pattern, data) # находит первый
# print(search)

# match = re.match(pattern, data)
# print(match)

# match_second = re.match(pattern, data_second)
# print(match_second)

# match_list = re.findall (r'\b\w{4}\b', 'Мама мыла раму, а папа был на пилораме, потому что работает на лесопилке.')
# print (match_list)