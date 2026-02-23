from __future__ import annotations

import sqlite3
import pandas as pd
from pathlib import Path

#TODO: подумать над архитектурой, где будет лучше себя чувствовать

class DatabaseManager:
    def __init__(self):
        self.base = Path(__file__).resolve().parent
        self.database = "database.db"
        self.openings_folder = str(Path(self.base).parent / "openings")
        self.conn: sqlite3.Connection | None = None
    
    # Определяю методы enter и exit для работы с with 

    def __enter__(self):
        print("Entered")
        self.conn = sqlite3.connect(self.database)
        return self

    def __exit__(self, exc_type, exc, tb):
        if self.conn:
            self.conn.close()

    def clear_opening_table(self):
        self.execute_write("DROP TABLE IF EXISTS openings;")

    # Делаю отдельный метод под запуск запросов
    def fetch_all(self, query: str, params: tuple = ()):
        cursor = self.conn.cursor()
        cursor.execute(query, params)
        return cursor.fetchall()
    
    def execute_write(self, query: str, params: tuple= ()):
        cursor = self.conn.cursor()
        cursor.execute(query, params)
        self.conn.commit()
        return cursor.rowcount

    def load_openings(self):
        for opening_list in Path(self.openings_folder).iterdir():
            df = pd.read_csv(opening_list, sep='\t')
            df.to_sql(self.database, self.conn, if_exists='append')
            # добавляю дебюты в базу данных (сначала нужно создать таблицу)
    
    def create_opening_table(self) -> None:
        self.clear_opening_table()
        
        self.execute_write("""CREATE TABLE IF NOT EXISTS openings (
                           id INTEGER PRIMARY KEY,
                           eco TEXT NOT NULL,
                           name TEXT NOT NULL,
                           pgn TEXT NOT NULL
                           );""")
        print("Opening table creation has been executed")

if __name__ == "__main__":
    with DatabaseManager() as database_manager:
        database_manager.create_opening_table()
        database_manager.load_openings()



