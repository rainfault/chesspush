class ChessPushIntroductoryService:
    CURRENT_VER = "0.1 ALPHA"
    
    def show_intro(self):
        print("Hello from chesspush!")
        print(f"Current version: {self.CURRENT_VER}")

        
if __name__ == "__main__":
    service = ChessPushIntroductoryService()
    service.show_intro()

