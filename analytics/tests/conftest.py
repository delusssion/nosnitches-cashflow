import sys
from pathlib import Path

ANALYTICS = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ANALYTICS / "scripts"), str(ANALYTICS / "reference")]
