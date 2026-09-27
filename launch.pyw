"""Windowless desktop entry point with a startup error log."""
from pathlib import Path
import contextlib
import os
import traceback

root = Path(__file__).resolve().parent
os.chdir(root)
log_path = root / 'data' / 'launch.log'
log_path.parent.mkdir(exist_ok=True)
if log_path.exists() and log_path.stat().st_size > 1024 * 1024:
    log_path.replace(log_path.with_suffix('.previous.log'))
with log_path.open('a', encoding='utf-8') as log, contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
    try:
        from main import main
        result = main()
        if result: raise RuntimeError(f'Qt startup failed ({result})')
    except Exception:
        traceback.print_exc()
        log.flush()
        if os.name == 'nt':
            import ctypes
            ctypes.windll.user32.MessageBoxW(None, f'Не удалось запустить ChessPush.\n{log_path}', 'ChessPush', 16)
