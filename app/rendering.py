"""Qt owns presentation pacing; Chromium's embedded 60 Hz limiter must not."""
import os


def configure_rendering():
    # Qt WebEngine 6.11 otherwise schedules RAF at 60 Hz even on a 144 Hz screen.
    # Keep Qt's native vsync enabled. Chessground only animates during interaction;
    # there is no permanent RAF loop in the application.
    flags = os.environ.get('QTWEBENGINE_CHROMIUM_FLAGS', '')
    if '--disable-frame-rate-limit' not in flags.split():
        os.environ['QTWEBENGINE_CHROMIUM_FLAGS'] = (flags + ' --disable-frame-rate-limit').strip()
