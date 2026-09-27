from __future__ import annotations

from PySide6.QtCore import Slot

from app.app_context import AppContext
from controllers.base import BaseController


class TrainingCardsController(BaseController):
    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self.refresh()

    @Slot()
    def refresh(self):
        self.set_rows(self.context.storage.list_training_cards())
        self.set_status("Карточки обновлены")

    @Slot(str, str, str, str, str, str, str, str)
    def addCard(self, title, fen, theme, problem, plan, typicalBreak, avoid, modelGames):
        title = title.strip()
        if not title:
            self.set_status("Название карточки обязательно")
            return
        card_id = self.context.storage.add_training_card(
            title=title,
            fen=fen.strip(),
            theme=theme.strip(),
            problem=problem.strip(),
            plan=plan.strip(),
            typical_break=typicalBreak.strip(),
            avoid=avoid.strip(),
            model_games=modelGames.strip(),
        )
        self.set_status(f"Карточка создана: #{card_id}")
        self.refresh()

    @Slot(int)
    def deleteCard(self, cardId):
        self.context.storage.delete_training_card(int(cardId))
        self.set_status(f"Карточка удалена: #{cardId}")
        self.refresh()
