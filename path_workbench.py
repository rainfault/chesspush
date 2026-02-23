from pathlib import Path
import sys


base = Path(__file__)

print(base)

resolved = Path(__file__).resolve()


print(resolved)
print (Path.cwd())
print(sys.executable)